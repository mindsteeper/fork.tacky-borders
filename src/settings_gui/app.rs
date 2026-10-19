//! The settings window itself: page navigation, load / save / apply, close handling

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use egui::collapsing_header::CollapsingState;
use egui::{Align, Color32, Layout, RichText, Ui, ViewportCommand};
use regex::Regex;

use super::editors::{self, hint, parse_angle, parse_hex};
use super::{WINDOW_TITLE, take_show_request};
use crate::colors::{ColorBrushConfig, GradientDirection};
use crate::config::{
    CONFIG_HOME_ENV, Config, ConfigDirSource, MatchKind, MatchStrategy, WindowRule,
    apply_config_if_changed,
};
use crate::komorebi::KomorebiColorsConfig;
use crate::render_backend::RenderBackendConfig;
use crate::utils::LogIfErr;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    General,
    Border,
    Colors,
    Animations,
    Effects,
    Rules,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::General,
        Page::Border,
        Page::Colors,
        Page::Animations,
        Page::Effects,
        Page::Rules,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::General => "일반",
            Page::Border => "테두리",
            Page::Colors => "색상",
            Page::Animations => "애니메이션",
            Page::Effects => "효과",
            Page::Rules => "창 규칙",
        }
    }
}

struct Status {
    text: String,
    is_error: bool,
}

pub struct SettingsApp {
    /// Config being edited
    config: Config,
    /// Last state read from or written to disk; differs from `config` when there are unsaved edits
    saved: Config,
    page: Page,
    status: Option<Status>,
    /// Shown instead of hiding when the close button is pressed with unsaved edits
    close_prompt: bool,
    /// Cleared by the apply thread once the borders have been recreated
    applying: Option<Arc<AtomicBool>>,
    /// Dirty state last reflected in the window title
    title_dirty: Option<bool>,
    /// Rule card to expand on the next frame (just added)
    expand_rule: Option<usize>,
}

impl SettingsApp {
    pub fn new() -> Self {
        let mut app = Self {
            config: Config::default(),
            saved: Config::default(),
            page: Page::General,
            status: None,
            close_prompt: false,
            applying: None,
            title_dirty: None,
            expand_rule: None,
        };
        app.load_from_disk();
        // The initial show request is already satisfied by the load above
        take_show_request();
        app
    }

    fn is_dirty(&self) -> bool {
        self.config != self.saved
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some(Status {
            text: text.into(),
            is_error,
        });
    }

    fn load_from_disk(&mut self) {
        match Config::create() {
            Ok(config) => {
                self.config = config.clone();
                self.saved = config;
                self.set_status("config.yaml 을 읽었습니다.", false);
            }
            Err(err) => {
                // Show a valid baseline so the broken file can be fixed by saving over it
                let fallback = Config::from_default_yaml().unwrap_or_default();
                self.config = fallback.clone();
                self.saved = fallback;
                self.set_status(
                    format!("config.yaml 을 읽지 못해 기본값을 표시합니다. 저장하면 파일을 덮어씁니다: {err:#}"),
                    true,
                );
            }
        }
    }

    fn save_and_apply(&mut self, ctx: &egui::Context) {
        let problems = validate(&self.config);
        if !problems.is_empty() {
            self.set_status(
                format!("저장할 수 없습니다: {}", problems.join(" / ")),
                true,
            );
            return;
        }

        if let Err(err) = self.config.save() {
            self.set_status(format!("저장 실패: {err:#}"), true);
            return;
        }

        self.saved = self.config.clone();

        // Recreating borders waits for their threads, so keep it off the GUI thread
        let applying = Arc::new(AtomicBool::new(true));
        let applying_clone = applying.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            apply_config_if_changed();
            applying_clone.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
        self.applying = Some(applying);
        self.set_status("저장했습니다. 테두리에 적용하는 중...", false);
    }

    fn hide(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = self.is_dirty();
        if self.title_dirty != Some(dirty) {
            let title = if dirty {
                format!("{WINDOW_TITLE} *")
            } else {
                WINDOW_TITLE.to_string()
            };
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
            self.title_dirty = Some(dirty);
        }
    }

    // --- layout ------------------------------------------------------------------------------

    fn nav(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("tacky-borders").heading());
        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
        ui.add_space(8.0);
        ui.separator();

        for page in Page::ALL {
            let label = RichText::new(page.title()).size(15.0);
            if ui.selectable_label(self.page == page, label).clicked() {
                self.page = page;
            }
        }
    }

    fn action_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let save_button = egui::Button::new(RichText::new("저장 및 적용").strong());
            if ui
                .add_enabled(self.applying.is_none(), save_button)
                .on_hover_text("config.yaml 에 저장하고 테두리를 다시 그립니다")
                .clicked()
            {
                self.save_and_apply(ctx);
            }

            if ui
                .button("파일에서 다시 읽기")
                .on_hover_text("편집 중인 내용을 버리고 config.yaml 을 다시 읽습니다")
                .clicked()
            {
                self.load_from_disk();
            }

            if ui
                .button("기본값으로 되돌리기")
                .on_hover_text("처음 실행 때 생성되는 기본 설정으로 채웁니다. 저장하기 전까지 파일은 그대로입니다")
                .clicked()
            {
                match Config::from_default_yaml() {
                    Ok(config) => {
                        self.config = config;
                        self.set_status("기본값을 불러왔습니다. 저장해야 적용됩니다.", false);
                    }
                    Err(err) => self.set_status(format!("기본값을 읽지 못했습니다: {err:#}"), true),
                }
            }

            if ui.button("설정 폴더 열기").clicked() {
                match Config::get_dir() {
                    Ok(dir) => open::that(dir).log_if_err(),
                    Err(err) => error!("{err:#}"),
                }
            }

            if self.is_dirty() {
                ui.label(RichText::new("● 저장하지 않은 변경").color(Color32::from_rgb(230, 160, 40)));
            }
        });

        if let Some(status) = &self.status {
            let color = if status.is_error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            ui.label(RichText::new(&status.text).color(color));
        }
        ui.add_space(4.0);
    }

    fn page_contents(&mut self, ui: &mut Ui) {
        match self.page {
            Page::General => self.page_general(ui),
            Page::Border => self.page_border(ui),
            Page::Colors => self.page_colors(ui),
            Page::Animations => self.page_animations(ui),
            Page::Effects => self.page_effects(ui),
            Page::Rules => self.page_rules(ui),
        }
    }

    // --- pages -------------------------------------------------------------------------------

    fn page_general(&mut self, ui: &mut Ui) {
        ui.heading("일반");
        ui.add_space(4.0);

        if let Ok(path) = Config::get_path() {
            ui.horizontal(|ui| {
                ui.label("설정 파일");
                ui.monospace(path.display().to_string());
            });
            match Config::get_dir_source() {
                Ok(ConfigDirSource::EnvVar) => {
                    hint(
                        ui,
                        &format!("환경 변수 {CONFIG_HOME_ENV} 로 지정한 폴더입니다"),
                    );
                }
                Ok(ConfigDirSource::ExeDir) => {
                    hint(
                        ui,
                        &format!(
                            "tacky-borders.exe 와 같은 폴더입니다. 환경 변수 {CONFIG_HOME_ENV} 로 바꿀 수 있습니다"
                        ),
                    );
                }
                Err(_) => {}
            }
        }
        ui.add_space(8.0);

        let config = &mut self.config;

        ui.checkbox(&mut config.watch_config_changes, "설정 파일 변경 감시")
            .on_hover_text("config.yaml 이 바뀌면 자동으로 테두리를 다시 불러옵니다. 텍스트 편집기로 직접 고칠 때 편리합니다.");

        ui.checkbox(&mut config.enable_logging, "로그 파일 기록")
            .on_hover_text("정보·경고·오류 메시지를 설정 폴더의 tacky-borders.log 에 기록합니다.");
        ui.indent("logging_hint", |ui| {
            hint(ui, "변경 시 tacky-borders 를 다시 시작해야 적용됩니다");
        });

        ui.checkbox(&mut config.enable_ipc_server, "IPC 서버 사용")
            .on_hover_text("실행 중인 tacky-borders 가 CLI 명령(tacky-borders.exe set-color 등)을 받도록 합니다.");
        ui.indent("ipc_hint", |ui| {
            hint(
                ui,
                "tacky-borders.exe settings 로 이 창을 여는 데에도 필요합니다",
            );
        });

        ui.add_space(12.0);
        ui.label(RichText::new("렌더러").strong());
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut config.render_backend,
                RenderBackendConfig::V2,
                "V2 (권장)",
            )
            .on_hover_text("기능이 많은 렌더러. 그림자·글로우 효과는 V2 에서만 동작합니다.");
            ui.radio_value(
                &mut config.render_backend,
                RenderBackendConfig::Legacy,
                "Legacy",
            )
            .on_hover_text("단순한 렌더러. V2 에서 문제가 생길 때 시도해 보세요.");
        });
        hint(ui, "성능은 PC 마다 다를 수 있으니 더 잘 맞는 쪽을 고르세요");
    }

    fn page_border(&mut self, ui: &mut Ui) {
        ui.heading("테두리");
        hint(
            ui,
            "모든 창에 적용되는 기본값입니다. 특정 창만 다르게 하려면 '창 규칙' 에서 덮어쓰세요",
        );
        ui.add_space(8.0);
        editors::border_grid(ui, &mut self.config.global);
    }

    fn page_colors(&mut self, ui: &mut Ui) {
        ui.heading("색상");
        ui.add_space(4.0);

        let global = &mut self.config.global;

        section(ui, "활성 창 (포커스된 창)", |ui| {
            editors::color_editor(ui, "active", &mut global.active_color, true);
        });
        section(ui, "비활성 창", |ui| {
            editors::color_editor(ui, "inactive", &mut global.inactive_color, true);
        });
        section(ui, "komorebi 연동", |ui| {
            editors::komorebi_editor(ui, &mut global.komorebi_colors);
        });
    }

    fn page_animations(&mut self, ui: &mut Ui) {
        ui.heading("애니메이션");
        ui.add_space(4.0);
        editors::animations_editor(ui, &mut self.config.global.animations);
    }

    fn page_effects(&mut self, ui: &mut Ui) {
        ui.heading("효과");
        ui.add_space(4.0);

        if self.config.render_backend != RenderBackendConfig::V2 {
            let color = ui.visuals().warn_fg_color;
            ui.label(RichText::new("렌더러가 Legacy 로 설정되어 있어 효과가 적용되지 않습니다. '일반' 에서 V2 로 바꾸세요.").color(color));
        }

        editors::effects_editor(ui, &mut self.config.global.effects);
    }

    fn page_rules(&mut self, ui: &mut Ui) {
        ui.heading("창 규칙");
        hint(
            ui,
            "창의 제목·클래스·프로세스 이름으로 창을 찾아, 그 창에만 다른 설정을 적용하거나 테두리를 끕니다. 위에서부터 먼저 일치하는 규칙 하나가 적용됩니다",
        );
        ui.add_space(8.0);

        let global = self.config.global.clone();
        let rules = &mut self.config.window_rules;
        let expand_rule = self.expand_rule.take();

        let mut action: Option<RuleAction> = None;
        let len = rules.len();

        for (i, rule) in rules.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let id = ui.make_persistent_id("rule_card");
                let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, false);
                if expand_rule == Some(i) {
                    state.set_open(true);
                }

                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    state
                        .show_header(ui, |ui| {
                            ui.label(RichText::new(editors::rule_title(rule)).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("삭제").clicked() {
                                    action = Some(RuleAction::Remove(i));
                                }
                                if ui
                                    .add_enabled(i + 1 < len, egui::Button::new("아래로").small())
                                    .clicked()
                                {
                                    action = Some(RuleAction::MoveDown(i));
                                }
                                if ui
                                    .add_enabled(i > 0, egui::Button::new("위로").small())
                                    .clicked()
                                {
                                    action = Some(RuleAction::MoveUp(i));
                                }
                            });
                        })
                        .body(|ui| editors::rule_editor(ui, rule, &global));
                });
            });
        }

        match action {
            Some(RuleAction::MoveUp(i)) if i > 0 => rules.swap(i, i - 1),
            Some(RuleAction::MoveDown(i)) if i + 1 < rules.len() => rules.swap(i, i + 1),
            Some(RuleAction::Remove(i)) if i < rules.len() => {
                rules.remove(i);
            }
            _ => {}
        }

        ui.add_space(4.0);
        if ui.button("+ 규칙 추가").clicked() {
            rules.push(WindowRule {
                kind: Some(MatchKind::Class),
                name: Some(String::new()),
                ..Default::default()
            });
            self.expand_rule = Some(rules.len() - 1);
        }
    }

    fn close_modal(&mut self, ctx: &egui::Context) {
        let modal = egui::Modal::new(egui::Id::new("close_prompt")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("저장하지 않은 변경이 있습니다");
            ui.label("창을 닫기 전에 변경 내용을 저장할까요?");
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("저장 후 닫기").clicked() {
                    self.save_and_apply(ctx);
                    if !self.is_dirty() {
                        self.close_prompt = false;
                        self.hide(ctx);
                    }
                }
                if ui.button("저장하지 않고 닫기").clicked() {
                    self.config = self.saved.clone();
                    self.close_prompt = false;
                    self.hide(ctx);
                }
                if ui.button("취소").clicked() {
                    self.close_prompt = false;
                }
            });
        });

        if modal.should_close() {
            self.close_prompt = false;
        }
    }
}

enum RuleAction {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

/// Titled group spanning the full width
fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        ui.separator();
        body(ui);
    });
    ui.add_space(6.0);
}

impl eframe::App for SettingsApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if take_show_request() {
            // Hand edits made while hidden should show up, but never at the cost of unsaved edits
            if !self.is_dirty() {
                self.load_from_disk();
            }
        }

        if let Some(applying) = &self.applying
            && !applying.load(Ordering::SeqCst)
        {
            self.applying = None;
            self.set_status("저장하고 테두리에 적용했습니다.", false);
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            // The window must never really close: the event loop could not be created again
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.is_dirty() {
                self.close_prompt = true;
            } else {
                self.hide(ctx);
            }
        }

        self.update_title(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

impl SettingsApp {
    /// Whole window layout; separate from App::ui so the render test can drive it without eframe
    fn render(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();

        egui::Panel::bottom("actions")
            .resizable(false)
            .show(ui, |ui| self.action_bar(ui, &ctx));

        egui::Panel::left("nav")
            .resizable(false)
            .default_size(160.0)
            .show(ui, |ui| self.nav(ui));

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| self.page_contents(ui));
        });

        if self.close_prompt {
            self.close_modal(&ctx);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Validation (runs before every save; a bad regex would otherwise panic the matcher)
// ---------------------------------------------------------------------------------------------

fn validate(config: &Config) -> Vec<String> {
    let mut problems = Vec::new();

    if config.global.border_width.0 < 0.0 {
        problems.push("테두리 두께는 0 이상이어야 합니다".to_string());
    }
    if config.global.animations.fps < 1 {
        problems.push("프레임 속도는 1 이상이어야 합니다".to_string());
    }
    check_color(&mut problems, "활성 창 색", &config.global.active_color);
    check_color(&mut problems, "비활성 창 색", &config.global.inactive_color);
    check_komorebi(&mut problems, "", &config.global.komorebi_colors);

    for (i, rule) in config.window_rules.iter().enumerate() {
        let prefix = format!("규칙 {}", i + 1);

        if rule.kind.is_none() {
            problems.push(format!("{prefix}: 찾기 기준을 고르세요"));
        }

        match rule.name.as_deref().map(str::trim) {
            None | Some("") => problems.push(format!("{prefix}: 이름이 비어 있습니다")),
            Some(name) => {
                if rule.strategy == Some(MatchStrategy::Regex) && Regex::new(name).is_err() {
                    problems.push(format!("{prefix}: 정규식이 잘못되었습니다"));
                }
            }
        }

        if let Some(color) = &rule.active_color {
            check_color(&mut problems, &format!("{prefix} 활성 창 색"), color);
        }
        if let Some(color) = &rule.inactive_color {
            check_color(&mut problems, &format!("{prefix} 비활성 창 색"), color);
        }
        if let Some(komorebi) = &rule.komorebi_colors {
            check_komorebi(&mut problems, &format!("{prefix} "), komorebi);
        }
        if let Some(animations) = &rule.animations
            && animations.fps < 1
        {
            problems.push(format!("{prefix}: 프레임 속도는 1 이상이어야 합니다"));
        }
    }

    problems
}

fn check_color(problems: &mut Vec<String>, label: &str, color: &ColorBrushConfig) {
    match color {
        ColorBrushConfig::Solid(hex) => check_hex(problems, label, hex),
        ColorBrushConfig::Gradient(gradient) => {
            if gradient.colors.len() < 2 {
                problems.push(format!("{label}: 그라데이션 색은 2개 이상이어야 합니다"));
            }
            for hex in &gradient.colors {
                check_hex(problems, label, hex);
            }
            if let GradientDirection::Angle(angle) = &gradient.direction
                && parse_angle(angle).is_none()
            {
                problems.push(format!("{label}: 각도 형식이 잘못되었습니다 ('{angle}')"));
            }
        }
        ColorBrushConfig::ThemeAware(theme) => {
            check_color(problems, &format!("{label} (다크)"), &theme.dark);
            check_color(problems, &format!("{label} (라이트)"), &theme.light);
        }
    }
}

fn check_hex(problems: &mut Vec<String>, label: &str, hex: &str) {
    if hex != "accent" && parse_hex(hex).is_none() {
        problems.push(format!("{label}: '{hex}' 는 올바른 색이 아닙니다"));
    }
}

fn check_komorebi(problems: &mut Vec<String>, prefix: &str, komorebi: &KomorebiColorsConfig) {
    let entries = [
        ("komorebi 스택 색", &komorebi.stack_color),
        ("komorebi 모노클 색", &komorebi.monocle_color),
        ("komorebi 플로팅 색", &komorebi.floating_color),
    ];
    for (label, color) in entries {
        if let Some(color) = color {
            check_color(problems, &format!("{prefix}{label}"), color);
        }
    }
}

#[cfg(test)]
mod render_tests {
    use std::path::PathBuf;

    use egui_kittest::Harness;

    use super::*;
    use crate::config::WidthConfig;

    /// Renders every page to target/settings_gui_snapshots/*.png for a visual check
    ///   - Needs a wgpu adapter (WARP is fine), so it is ignored by default:
    ///     cargo test --lib settings_gui -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_pages_to_png() {
        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("settings_gui_snapshots");
        let config_dir = out_dir.join("config");
        std::fs::create_dir_all(&config_dir).unwrap();

        // SAFETY: no other thread in this test binary reads the env var concurrently
        unsafe { std::env::set_var("TACKY_BORDERS_CONFIG_HOME", &config_dir) };

        let mut app = SettingsApp::new();
        // Exercise the override editors on the first rule
        if let Some(rule) = app.config.window_rules.first_mut() {
            rule.border_width = Some(WidthConfig::new(2.0));
            rule.active_color = Some(app.config.global.active_color.clone());
        }

        let mut harness = Harness::builder()
            .with_size(egui::vec2(920.0, 680.0))
            .wgpu()
            .build_ui_state(|ui, app: &mut SettingsApp| app.render(ui), app);
        super::super::setup_style(&harness.ctx);

        for page in Page::ALL {
            harness.state_mut().page = page;
            if page == Page::Rules {
                harness.state_mut().expand_rule = Some(0);
            }
            harness.run();

            let image = harness.render().expect("could not render page");
            let path = out_dir.join(format!("{}.png", format!("{page:?}").to_lowercase()));
            image.save(&path).expect("could not save png");
            println!("saved {}", path.display());
        }

        harness.state_mut().close_prompt = true;
        harness.run();
        let image = harness.render().expect("could not render modal");
        let path = out_dir.join("close_prompt.png");
        image.save(&path).expect("could not save png");
        println!("saved {}", path.display());
    }
}
