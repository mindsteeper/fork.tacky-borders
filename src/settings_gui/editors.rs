//! Reusable editors for the config types, shared by the global pages and the window rules
//!   - Every editor mutates the config value in place; nothing is cached between frames

use egui::color_picker::Alpha;
use egui::{Align, Button, DragValue, Layout, RichText, TextEdit, TextStyle, Ui};

use crate::animations::{AnimEasing, AnimParamsConfig, AnimType, AnimationsConfig};
use crate::colors::{
    ColorBrushConfig, GradientBrushConfig, GradientCoordinates, GradientDirection, ThemeAwareColor,
};
use crate::config::{
    EnableMode, Global, MatchKind, MatchStrategy, Offset, OffsetConfig, RadiusConfig, WidthConfig,
    WindowRule, ZOrderMode,
};
use crate::effects::{EffectParamsConfig, EffectType, EffectsConfig, Translation};
use crate::komorebi::KomorebiColorsConfig;

const FALLBACK_HEX: &str = "#6274e7";

// ---------------------------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------------------------

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

fn error_label(ui: &mut Ui, text: &str) {
    let color = ui.visuals().error_fg_color;
    ui.label(RichText::new(text).small().color(color));
}

/// Combo box over a fixed option list. Returns true when the selection changed.
pub fn enum_combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let current = options
        .iter()
        .find(|(option, _)| option == value)
        .map(|(_, label)| *label)
        .unwrap_or("?");

    let mut changed = false;
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (option, label) in options {
                if ui.selectable_value(value, *option, *label).changed() {
                    changed = true;
                }
            }
        });

    changed
}

/// Checkbox that toggles an Option on (with a default) or off, showing the editor while on
pub fn optional_field<T>(
    ui: &mut Ui,
    label: &str,
    value: &mut Option<T>,
    default: impl FnOnce() -> T,
    editor: impl FnOnce(&mut Ui, &mut T),
) {
    ui.push_id(label, |ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, label).changed() {
            *value = if enabled { Some(default()) } else { None };
        }

        if let Some(inner) = value.as_mut() {
            ui.indent("body", |ui| editor(ui, inner));
            ui.add_space(4.0);
        }
    });
}

enum ListAction {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

fn list_buttons(ui: &mut Ui, index: usize, len: usize, action: &mut Option<ListAction>) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.small_button("삭제").clicked() {
            *action = Some(ListAction::Remove(index));
        }
        if ui
            .add_enabled(index + 1 < len, Button::new("아래로").small())
            .clicked()
        {
            *action = Some(ListAction::MoveDown(index));
        }
        if ui
            .add_enabled(index > 0, Button::new("위로").small())
            .clicked()
        {
            *action = Some(ListAction::MoveUp(index));
        }
    });
}

fn apply_list_action<T>(list: &mut Vec<T>, action: Option<ListAction>) {
    match action {
        Some(ListAction::MoveUp(i)) if i > 0 => list.swap(i, i - 1),
        Some(ListAction::MoveDown(i)) if i + 1 < list.len() => list.swap(i, i + 1),
        Some(ListAction::Remove(i)) if i < list.len() => {
            list.remove(i);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Color parsing (same formats as colors.rs: #RGB, #RGBA, #RRGGBB, #RRGGBBAA)
// ---------------------------------------------------------------------------------------------

pub fn parse_hex(text: &str) -> Option<egui::Color32> {
    let digits = text.strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let single = |i: usize| u8::from_str_radix(&digits[i..i + 1], 16).map(|n| (n << 4) | n);
    let double = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16);

    let [r, g, b, a] = match digits.len() {
        3 => [single(0), single(1), single(2), Ok(255)],
        4 => [single(0), single(1), single(2), single(3)],
        6 => [double(0), double(2), double(4), Ok(255)],
        8 => [double(0), double(2), double(4), double(6)],
        _ => return None,
    };

    Some(egui::Color32::from_rgba_unmultiplied(
        r.ok()?,
        g.ok()?,
        b.ok()?,
        a.ok()?,
    ))
}

pub fn format_hex(color: egui::Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// "45deg" -> 45.0 (same rule as colors.rs)
pub fn parse_angle(text: &str) -> Option<f32> {
    text.strip_suffix("deg")
        .and_then(|degrees| degrees.trim().parse::<f32>().ok())
}

// ---------------------------------------------------------------------------------------------
// Border basics
// ---------------------------------------------------------------------------------------------

pub fn width_editor(ui: &mut Ui, width: &mut WidthConfig) {
    ui.horizontal(|ui| {
        ui.add(
            DragValue::new(&mut width.0)
                .range(0.0..=100.0)
                .speed(0.25)
                .max_decimals(2)
                .suffix(" px"),
        );
        hint(ui, "DPI 배율에 맞춰 자동으로 환산됩니다");
    });
}

pub fn offset_editor(ui: &mut Ui, offset: &mut OffsetConfig) {
    ui.vertical(|ui| {
        let uniform = matches!(offset, OffsetConfig::Uniform(_));

        ui.horizontal(|ui| {
            if ui.radio(uniform, "모든 면 같게").clicked() && !uniform {
                let value = match offset {
                    OffsetConfig::PerSide(sides) => sides.top,
                    OffsetConfig::Uniform(value) => *value,
                };
                *offset = OffsetConfig::Uniform(value);
            }
            if ui.radio(!uniform, "면마다 다르게").clicked() && uniform {
                let value = match offset {
                    OffsetConfig::Uniform(value) => *value,
                    OffsetConfig::PerSide(sides) => sides.top,
                };
                *offset = OffsetConfig::PerSide(Offset::new(value));
            }
        });

        match offset {
            OffsetConfig::Uniform(value) => {
                ui.add(DragValue::new(value).range(-50..=50).suffix(" px"));
            }
            OffsetConfig::PerSide(sides) => {
                egui::Grid::new("offset_sides")
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("위");
                        ui.add(DragValue::new(&mut sides.top).range(-50..=50).suffix(" px"));
                        ui.label("아래");
                        ui.add(
                            DragValue::new(&mut sides.bottom)
                                .range(-50..=50)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label("왼쪽");
                        ui.add(
                            DragValue::new(&mut sides.left)
                                .range(-50..=50)
                                .suffix(" px"),
                        );
                        ui.label("오른쪽");
                        ui.add(
                            DragValue::new(&mut sides.right)
                                .range(-50..=50)
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
            }
        }

        hint(
            ui,
            "음수면 창 안쪽으로, 양수면 창 바깥쪽으로 테두리를 옮깁니다",
        );
    });
}

pub fn radius_editor(ui: &mut Ui, radius: &mut RadiusConfig) {
    #[derive(PartialEq, Clone, Copy)]
    enum Choice {
        Auto,
        Square,
        Round,
        RoundSmall,
        Custom,
    }

    let mut choice = match radius {
        RadiusConfig::Auto => Choice::Auto,
        RadiusConfig::Square => Choice::Square,
        RadiusConfig::Round => Choice::Round,
        RadiusConfig::RoundSmall => Choice::RoundSmall,
        RadiusConfig::Custom(_) => Choice::Custom,
    };

    ui.horizontal(|ui| {
        let changed = enum_combo(
            ui,
            "radius",
            &mut choice,
            &[
                (Choice::Auto, "자동 (창의 모서리 설정을 따름)"),
                (Choice::Square, "직각"),
                (Choice::Round, "둥글게"),
                (Choice::RoundSmall, "조금 둥글게"),
                (Choice::Custom, "직접 지정"),
            ],
        );

        if changed {
            *radius = match choice {
                Choice::Auto => RadiusConfig::Auto,
                Choice::Square => RadiusConfig::Square,
                Choice::Round => RadiusConfig::Round,
                Choice::RoundSmall => RadiusConfig::RoundSmall,
                Choice::Custom => RadiusConfig::Custom(8.0),
            };
        }

        if let RadiusConfig::Custom(value) = radius {
            ui.add(
                DragValue::new(value)
                    .range(0.0..=200.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" px"),
            );
        }
    });
}

pub fn z_order_editor(ui: &mut Ui, z_order: &mut ZOrderMode) {
    ui.vertical(|ui| {
        enum_combo(
            ui,
            "z_order",
            z_order,
            &[
                (ZOrderMode::AboveWindow, "창 위에 (기본)"),
                (ZOrderMode::BelowWindow, "창 아래에"),
            ],
        );
        hint(
            ui,
            "창 위에 두면 창 순서를 바꾸는 일부 앱에서 테두리가 깜빡일 수 있습니다",
        );
    });
}

pub fn follow_native_editor(ui: &mut Ui, follow: &mut bool) {
    ui.vertical(|ui| {
        ui.checkbox(follow, "Windows 기본 테두리의 표시 여부를 따라감");
        hint(
            ui,
            "예: 테두리 없는 전체화면 창에서는 Windows 처럼 테두리를 숨깁니다",
        );
    });
}

pub fn delay_editor(ui: &mut Ui, delay: &mut u64) {
    ui.add(DragValue::new(delay).range(0..=5000).speed(5).suffix(" ms"));
}

/// Label + editor rows for everything in Global except colors, animations and effects
pub fn border_grid(ui: &mut Ui, global: &mut Global) {
    egui::Grid::new("border_grid")
        .num_columns(2)
        .spacing([24.0, 12.0])
        .show(ui, |ui| {
            ui.label("두께");
            width_editor(ui, &mut global.border_width);
            ui.end_row();

            ui.label("위치 (오프셋)");
            offset_editor(ui, &mut global.border_offset);
            ui.end_row();

            ui.label("모서리");
            radius_editor(ui, &mut global.border_radius);
            ui.end_row();

            ui.label("Z 순서");
            z_order_editor(ui, &mut global.border_z_order);
            ui.end_row();

            ui.label("기본 테두리 따라가기");
            follow_native_editor(ui, &mut global.follow_native_border);
            ui.end_row();

            ui.label("새 창 지연");
            ui.vertical(|ui| {
                delay_editor(ui, &mut global.initialize_delay);
                hint(ui, "창이 열린 뒤 테두리가 나타나기까지 기다리는 시간. 창 열림 애니메이션을 가리지 않게 합니다");
            });
            ui.end_row();

            ui.label("복원 지연");
            ui.vertical(|ui| {
                delay_editor(ui, &mut global.unminimize_delay);
                hint(ui, "최소화를 해제한 뒤 테두리가 나타나기까지 기다리는 시간");
            });
            ui.end_row();
        });
}

// ---------------------------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum ColorKind {
    Solid,
    Accent,
    Gradient,
    ThemeAware,
}

impl ColorKind {
    fn of(color: &ColorBrushConfig) -> Self {
        match color {
            ColorBrushConfig::Solid(hex) if hex == "accent" => Self::Accent,
            ColorBrushConfig::Solid(_) => Self::Solid,
            ColorBrushConfig::Gradient(_) => Self::Gradient,
            ColorBrushConfig::ThemeAware(_) => Self::ThemeAware,
        }
    }

    /// Converts between kinds while keeping a representative color where possible
    fn convert(self, from: &ColorBrushConfig) -> ColorBrushConfig {
        match self {
            Self::Solid => ColorBrushConfig::Solid(first_hex(from)),
            Self::Accent => ColorBrushConfig::Solid("accent".to_string()),
            Self::Gradient => match from {
                ColorBrushConfig::Gradient(_) => from.clone(),
                _ => ColorBrushConfig::Gradient(GradientBrushConfig {
                    colors: vec![first_hex(from), "#ffffff".to_string()],
                    direction: GradientDirection::Angle("45deg".to_string()),
                }),
            },
            Self::ThemeAware => match from {
                ColorBrushConfig::ThemeAware(_) => from.clone(),
                _ => ColorBrushConfig::ThemeAware(ThemeAwareColor {
                    dark: Box::new(from.clone()),
                    light: Box::new(from.clone()),
                }),
            },
        }
    }
}

fn first_hex(color: &ColorBrushConfig) -> String {
    match color {
        ColorBrushConfig::Solid(hex) if hex != "accent" => hex.clone(),
        ColorBrushConfig::Solid(_) => FALLBACK_HEX.to_string(),
        ColorBrushConfig::Gradient(gradient) => gradient
            .colors
            .iter()
            .find(|c| *c != "accent")
            .cloned()
            .unwrap_or_else(|| FALLBACK_HEX.to_string()),
        ColorBrushConfig::ThemeAware(theme) => first_hex(&theme.dark),
    }
}

/// Color swatch + hex text field. "accent" is accepted as text and shown as a label instead.
fn hex_color_row(ui: &mut Ui, hex: &mut String) {
    let parsed = parse_hex(hex);

    if hex == "accent" {
        ui.label("강조색");
    } else if let Some(mut color) = parsed
        && egui::color_picker::color_edit_button_srgba(ui, &mut color, Alpha::OnlyBlend).changed()
    {
        *hex = format_hex(color);
    }

    ui.add(
        TextEdit::singleline(hex)
            .desired_width(100.0)
            .font(TextStyle::Monospace),
    );

    if parsed.is_none() && hex != "accent" {
        error_label(ui, "잘못된 색 (#RRGGBB 또는 #RRGGBBAA)");
    }
}

pub fn color_editor(ui: &mut Ui, id: &str, color: &mut ColorBrushConfig, allow_theme_aware: bool) {
    ui.push_id(id, |ui| {
        let mut kind = ColorKind::of(color);

        ui.horizontal(|ui| {
            ui.label("종류");
            let mut options = vec![
                (ColorKind::Solid, "단색"),
                (ColorKind::Accent, "Windows 강조색"),
                (ColorKind::Gradient, "그라데이션"),
            ];
            if allow_theme_aware {
                options.push((ColorKind::ThemeAware, "테마별 (다크 / 라이트)"));
            }
            if enum_combo(ui, "kind", &mut kind, &options) {
                let converted = kind.convert(color);
                *color = converted;
            }
        });

        match color {
            ColorBrushConfig::Solid(hex) if hex == "accent" => {
                hint(ui, "Windows 설정 > 개인 설정 > 색 의 강조색을 씁니다. 비활성 창에는 흐린 톤이 적용됩니다");
            }
            ColorBrushConfig::Solid(hex) => {
                ui.horizontal(|ui| {
                    ui.label("색");
                    hex_color_row(ui, hex);
                });
            }
            ColorBrushConfig::Gradient(gradient) => gradient_editor(ui, gradient),
            ColorBrushConfig::ThemeAware(theme) => {
                hint(ui, "Windows 테마(다크/라이트)가 바뀌면 자동으로 전환됩니다");
                ui.label(RichText::new("다크 모드").strong());
                ui.indent("dark", |ui| color_editor(ui, "dark", &mut theme.dark, false));
                ui.label(RichText::new("라이트 모드").strong());
                ui.indent("light", |ui| color_editor(ui, "light", &mut theme.light, false));
            }
        }
    });
}

fn gradient_editor(ui: &mut Ui, gradient: &mut GradientBrushConfig) {
    ui.label("색 목록 (순서대로, 2개 이상)");

    let mut action = None;
    let len = gradient.colors.len();
    for i in 0..len {
        ui.push_id(i, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", i + 1));
                hex_color_row(ui, &mut gradient.colors[i]);
                if ui
                    .add_enabled(len > 2, Button::new("삭제").small())
                    .clicked()
                {
                    action = Some(ListAction::Remove(i));
                }
            });
        });
    }
    apply_list_action(&mut gradient.colors, action);

    ui.horizontal(|ui| {
        if ui.button("+ 색 추가").clicked() {
            let next = gradient
                .colors
                .last()
                .filter(|c| *c != "accent")
                .cloned()
                .unwrap_or_else(|| "#ffffff".to_string());
            gradient.colors.push(next);
        }
        hint(ui, "색 칸에 accent 라고 쓰면 Windows 강조색이 됩니다");
    });

    ui.add_space(4.0);
    ui.label("방향");

    let is_angle = matches!(gradient.direction, GradientDirection::Angle(_));
    ui.horizontal(|ui| {
        if ui.radio(is_angle, "각도").clicked() && !is_angle {
            gradient.direction = GradientDirection::Angle("45deg".to_string());
        }
        if ui.radio(!is_angle, "좌표").clicked() && is_angle {
            gradient.direction = GradientDirection::Coordinates(GradientCoordinates {
                start: [0.0, 1.0],
                end: [1.0, 0.0],
            });
        }
    });

    match &mut gradient.direction {
        GradientDirection::Angle(text) => {
            let parsed = parse_angle(text);
            let mut degrees = parsed.unwrap_or(0.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        DragValue::new(&mut degrees)
                            .range(-360.0..=360.0)
                            .speed(1.0)
                            .max_decimals(1)
                            .suffix("°"),
                    )
                    .changed()
                {
                    *text = format!("{degrees}deg");
                }
                hint(ui, "0° = 왼쪽→오른쪽, 90° = 아래→위 (반시계 방향으로 커짐)");
            });
            if parsed.is_none() {
                error_label(ui, "각도 형식이 잘못되었습니다 (예: 45deg)");
            }
        }
        GradientDirection::Coordinates(coords) => {
            egui::Grid::new("gradient_coords")
                .num_columns(5)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("시작");
                    ui.label("x");
                    ui.add(coord_drag(&mut coords.start[0]));
                    ui.label("y");
                    ui.add(coord_drag(&mut coords.start[1]));
                    ui.end_row();
                    ui.label("끝");
                    ui.label("x");
                    ui.add(coord_drag(&mut coords.end[0]));
                    ui.label("y");
                    ui.add(coord_drag(&mut coords.end[1]));
                    ui.end_row();
                });
            hint(ui, "[0, 0] = 왼쪽 위, [1, 1] = 오른쪽 아래");
        }
    }
}

fn coord_drag(value: &mut f32) -> DragValue<'_> {
    DragValue::new(value)
        .range(0.0..=1.0)
        .speed(0.01)
        .fixed_decimals(2)
}

pub fn komorebi_editor(ui: &mut Ui, komorebi: &mut KomorebiColorsConfig) {
    ui.checkbox(&mut komorebi.enabled, "komorebi 연동 사용")
        .on_hover_text("komorebi 타일링 창 관리자의 창 종류(스택 / 모노클 / 플로팅)에 따라 다른 색을 씁니다. 단일(Single) 창은 활성 색, 비포커스 창은 비활성 색을 씁니다.");

    ui.add_enabled_ui(komorebi.enabled, |ui| {
        optional_field(
            ui,
            "스택(Stack) 창 색",
            &mut komorebi.stack_color,
            || ColorBrushConfig::Solid("#e762b7".to_string()),
            |ui, color| color_editor(ui, "stack", color, true),
        );
        optional_field(
            ui,
            "모노클(Monocle) 창 색",
            &mut komorebi.monocle_color,
            || ColorBrushConfig::Solid("#62e793".to_string()),
            |ui, color| color_editor(ui, "monocle", color, true),
        );
        optional_field(
            ui,
            "플로팅(Floating) 창 색",
            &mut komorebi.floating_color,
            || ColorBrushConfig::Solid("#f5f5a5".to_string()),
            |ui, color| color_editor(ui, "floating", color, true),
        );
    });
}

// ---------------------------------------------------------------------------------------------
// Animations
// ---------------------------------------------------------------------------------------------

const NAMED_EASINGS: [AnimEasing; 27] = [
    AnimEasing::Linear,
    AnimEasing::EaseIn,
    AnimEasing::EaseInSine,
    AnimEasing::EaseInQuad,
    AnimEasing::EaseInCubic,
    AnimEasing::EaseInQuart,
    AnimEasing::EaseInQuint,
    AnimEasing::EaseInExpo,
    AnimEasing::EaseInCirc,
    AnimEasing::EaseInBack,
    AnimEasing::EaseOut,
    AnimEasing::EaseOutSine,
    AnimEasing::EaseOutQuad,
    AnimEasing::EaseOutCubic,
    AnimEasing::EaseOutQuart,
    AnimEasing::EaseOutQuint,
    AnimEasing::EaseOutExpo,
    AnimEasing::EaseOutCirc,
    AnimEasing::EaseOutBack,
    AnimEasing::EaseInOut,
    AnimEasing::EaseInOutSine,
    AnimEasing::EaseInOutQuad,
    AnimEasing::EaseInOutCubic,
    AnimEasing::EaseInOutQuart,
    AnimEasing::EaseInOutQuint,
    AnimEasing::EaseInOutExpo,
    AnimEasing::EaseInOutCirc,
];

fn default_duration(anim_type: AnimType) -> f32 {
    match anim_type {
        AnimType::Spiral | AnimType::ReverseSpiral => 1800.0,
        AnimType::Fade => 200.0,
    }
}

pub fn animations_editor(ui: &mut Ui, animations: &mut AnimationsConfig) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut animations.enabled, "애니메이션 사용");
        ui.add_space(16.0);
        ui.label("프레임 속도");
        ui.add(
            DragValue::new(&mut animations.fps)
                .range(1..=240)
                .suffix(" fps"),
        );
    });
    hint(
        ui,
        "Spiral 계열은 그라데이션 색을 회전시키고, Fade 는 활성/비활성 전환을 부드럽게 합니다. Spiral 은 저사양 PC 에서 CPU 를 많이 쓸 수 있습니다",
    );

    ui.add_enabled_ui(animations.enabled, |ui| {
        anim_list(ui, "활성 창", "active", &mut animations.active);
        anim_list(ui, "비활성 창", "inactive", &mut animations.inactive);
    });
}

fn anim_list(ui: &mut Ui, title: &str, id: &str, list: &mut Vec<AnimParamsConfig>) {
    ui.push_id(id, |ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong());

        if list.is_empty() {
            hint(ui, "애니메이션 없음");
        }

        let mut action = None;
        let len = list.len();
        for (i, item) in list.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        enum_combo(
                            ui,
                            "type",
                            &mut item.anim_type,
                            &[
                                (AnimType::Spiral, "Spiral (회전)"),
                                (AnimType::ReverseSpiral, "ReverseSpiral (역회전)"),
                                (AnimType::Fade, "Fade (서서히 전환)"),
                            ],
                        );
                        list_buttons(ui, i, len, &mut action);
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(20.0);
                        ui.label("길이");
                        optional_number(
                            ui,
                            &mut item.duration,
                            default_duration(item.anim_type),
                            " ms",
                            10.0..=20000.0,
                            10.0,
                        );

                        ui.add_space(8.0);
                        ui.label("가속 곡선");
                        easing_editor(ui, &mut item.easing);
                    });
                });
            });
        }
        apply_list_action(list, action);

        if ui.button("+ 애니메이션 추가").clicked() {
            list.push(AnimParamsConfig {
                anim_type: AnimType::Fade,
                duration: None,
                easing: None,
            });
        }
    });
}

/// Checkbox = "직접 지정"; unchecked shows the built-in default instead of a field
fn optional_number(
    ui: &mut Ui,
    value: &mut Option<f32>,
    default: f32,
    suffix: &str,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
) {
    let mut custom = value.is_some();
    if ui
        .checkbox(&mut custom, "")
        .on_hover_text("체크하면 직접 지정, 해제하면 기본값 사용")
        .changed()
    {
        *value = custom.then_some(default);
    }

    match value {
        Some(inner) => {
            ui.add(
                DragValue::new(inner)
                    .range(range)
                    .speed(speed)
                    .max_decimals(0)
                    .suffix(suffix),
            );
        }
        None => {
            ui.label(RichText::new(format!("기본 {default}{suffix}")).weak());
        }
    }
}

fn easing_editor(ui: &mut Ui, easing: &mut Option<AnimEasing>) {
    #[derive(PartialEq, Clone, Copy)]
    enum Choice {
        Default,
        Named(AnimEasing),
        Bezier,
    }

    let mut choice = match easing {
        None => Choice::Default,
        Some(AnimEasing::CubicBezier(_)) => Choice::Bezier,
        Some(named) => Choice::Named(*named),
    };

    let selected_text = match choice {
        Choice::Default => "기본 (Linear)".to_string(),
        Choice::Named(named) => format!("{named:?}"),
        Choice::Bezier => "CubicBezier (직접 입력)".to_string(),
    };

    egui::ComboBox::from_id_salt("easing")
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut choice, Choice::Default, "기본 (Linear)");
            for named in NAMED_EASINGS {
                ui.selectable_value(&mut choice, Choice::Named(named), format!("{named:?}"));
            }
            ui.selectable_value(&mut choice, Choice::Bezier, "CubicBezier (직접 입력)");
        });

    match choice {
        Choice::Default => *easing = None,
        Choice::Named(named) => *easing = Some(named),
        Choice::Bezier => {
            if !matches!(easing, Some(AnimEasing::CubicBezier(_))) {
                *easing = Some(AnimEasing::CubicBezier([0.25, 0.1, 0.25, 1.0]));
            }
        }
    }

    if let Some(AnimEasing::CubicBezier(points)) = easing {
        for point in points.iter_mut() {
            ui.add(
                DragValue::new(point)
                    .range(-2.0..=2.0)
                    .speed(0.01)
                    .fixed_decimals(2),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------------------------

pub fn effects_editor(ui: &mut Ui, effects: &mut EffectsConfig) {
    ui.checkbox(&mut effects.enabled, "효과 사용");
    hint(
        ui,
        "그림자(Shadow)와 글로우(Glow)는 CPU/GPU 사용량을 크게 늘릴 수 있습니다. 렌더러가 V2 일 때만 동작합니다",
    );

    ui.add_enabled_ui(effects.enabled, |ui| {
        effect_list(ui, "활성 창", "active", &mut effects.active);
        effect_list(ui, "비활성 창", "inactive", &mut effects.inactive);
    });
}

fn effect_list(ui: &mut Ui, title: &str, id: &str, list: &mut Vec<EffectParamsConfig>) {
    ui.push_id(id, |ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong());

        if list.is_empty() {
            hint(ui, "효과 없음");
        }

        let mut action = None;
        let len = list.len();
        for (i, item) in list.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        enum_combo(
                            ui,
                            "type",
                            &mut item.effect_type,
                            &[
                                (EffectType::Shadow, "Shadow (그림자)"),
                                (EffectType::Glow, "Glow (글로우)"),
                            ],
                        );
                        list_buttons(ui, i, len, &mut action);
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(20.0);
                        ui.label("흐림");
                        ui.add(
                            DragValue::new(&mut item.std_dev)
                                .range(0.0..=100.0)
                                .speed(0.1)
                                .max_decimals(1),
                        )
                        .on_hover_text("흐림 정도 (가우시안 표준편차)");

                        ui.label("퍼짐");
                        ui.add(
                            DragValue::new(&mut item.spread)
                                .range(-50.0..=50.0)
                                .speed(0.1)
                                .max_decimals(1),
                        )
                        .on_hover_text("효과를 테두리보다 얼마나 키울지");

                        ui.label("불투명도");
                        ui.add(
                            DragValue::new(&mut item.opacity)
                                .range(0.0..=1.0)
                                .speed(0.01)
                                .fixed_decimals(2),
                        );

                        ui.label("이동 x");
                        ui.add(
                            DragValue::new(&mut item.translation.x)
                                .range(-100.0..=100.0)
                                .speed(0.5)
                                .max_decimals(1),
                        );
                        ui.label("y");
                        ui.add(
                            DragValue::new(&mut item.translation.y)
                                .range(-100.0..=100.0)
                                .speed(0.5)
                                .max_decimals(1),
                        )
                        .on_hover_text(
                            "효과를 옮길 거리 (px). 그림자를 아래로 떨어뜨릴 때 y 를 키웁니다",
                        );
                    });
                });
            });
        }
        apply_list_action(list, action);

        if ui.button("+ 효과 추가").clicked() {
            list.push(EffectParamsConfig {
                effect_type: EffectType::Glow,
                std_dev: 8.0,
                spread: 0.0,
                opacity: 1.0,
                translation: Translation::default(),
            });
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Window rules
// ---------------------------------------------------------------------------------------------

pub fn rule_title(rule: &WindowRule) -> String {
    let kind = match rule.kind {
        Some(MatchKind::Title) => "제목",
        Some(MatchKind::Class) => "클래스",
        Some(MatchKind::Process) => "프로세스",
        None => "(기준 없음)",
    };
    let name = rule
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("(이름 없음)");
    let state = match rule.enabled {
        Some(EnableMode::Bool(false)) => "  ·  테두리 끔",
        Some(EnableMode::Bool(true)) => "  ·  항상 켬",
        _ => "",
    };

    format!("{kind}: {name}{state}")
}

pub fn rule_editor(ui: &mut Ui, rule: &mut WindowRule, global: &Global) {
    egui::Grid::new("rule_match")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label("찾기 기준");
            enum_combo(
                ui,
                "kind",
                &mut rule.kind,
                &[
                    (Some(MatchKind::Title), "창 제목"),
                    (Some(MatchKind::Class), "창 클래스"),
                    (Some(MatchKind::Process), "프로세스 이름"),
                ],
            );
            ui.end_row();

            ui.label("이름");
            ui.vertical(|ui| {
                let name = rule.name.get_or_insert_with(String::new);
                ui.add(TextEdit::singleline(name).desired_width(320.0));
                let name_hint = match rule.kind {
                    Some(MatchKind::Title) => "창 제목 표시줄의 글자 (대소문자 구분 없음)",
                    Some(MatchKind::Class) => {
                        "창 클래스 이름 (예: MozillaWindowClass, CabinetWClass)"
                    }
                    Some(MatchKind::Process) => {
                        "실행 파일 이름에서 .exe 를 뺀 것 (예: chrome, Code)"
                    }
                    None => "먼저 찾기 기준을 고르세요",
                };
                hint(ui, name_hint);
            });
            ui.end_row();

            ui.label("비교 방식");
            {
                // None means Equals at runtime, so the combo never needs a separate None entry
                let mut strategy = rule.strategy.unwrap_or(MatchStrategy::Equals);
                if enum_combo(
                    ui,
                    "strategy",
                    &mut strategy,
                    &[
                        (MatchStrategy::Equals, "같음"),
                        (MatchStrategy::Contains, "포함"),
                        (MatchStrategy::Regex, "정규식"),
                    ],
                ) {
                    rule.strategy = Some(strategy);
                }
            }
            ui.end_row();

            ui.label("테두리");
            {
                // None and Auto behave the same; keep None so the saved file stays minimal
                let mut enabled = match rule.enabled {
                    Some(EnableMode::Bool(value)) => Some(value),
                    _ => None,
                };
                if enum_combo(
                    ui,
                    "enabled",
                    &mut enabled,
                    &[(None, "자동"), (Some(true), "항상 켬"), (Some(false), "끔")],
                ) {
                    rule.enabled = enabled.map(EnableMode::Bool);
                }
            }
            ui.end_row();
        });
    hint(
        ui,
        "자동: 일반 창에만 테두리를 그림 / 항상 켬: 도구 창 등도 강제로 그림 / 끔: 이 창에는 그리지 않음",
    );

    ui.add_space(8.0);
    ui.label(RichText::new("이 창에만 다르게 적용할 설정").strong());
    hint(ui, "체크한 항목만 전역 설정 대신 이 규칙의 값이 쓰입니다");

    optional_field(
        ui,
        "두께",
        &mut rule.border_width,
        || global.border_width,
        width_editor,
    );
    optional_field(
        ui,
        "위치 (오프셋)",
        &mut rule.border_offset,
        || global.border_offset,
        offset_editor,
    );
    optional_field(
        ui,
        "모서리",
        &mut rule.border_radius,
        || global.border_radius,
        radius_editor,
    );
    optional_field(
        ui,
        "Z 순서",
        &mut rule.border_z_order,
        || global.border_z_order,
        z_order_editor,
    );
    optional_field(
        ui,
        "기본 테두리 따라가기",
        &mut rule.follow_native_border,
        || global.follow_native_border,
        follow_native_editor,
    );
    optional_field(
        ui,
        "활성 창 색",
        &mut rule.active_color,
        || global.active_color.clone(),
        |ui, color| color_editor(ui, "active", color, true),
    );
    optional_field(
        ui,
        "비활성 창 색",
        &mut rule.inactive_color,
        || global.inactive_color.clone(),
        |ui, color| color_editor(ui, "inactive", color, true),
    );
    optional_field(
        ui,
        "komorebi 색",
        &mut rule.komorebi_colors,
        || global.komorebi_colors.clone(),
        komorebi_editor,
    );
    optional_field(
        ui,
        "애니메이션",
        &mut rule.animations,
        || global.animations.clone(),
        animations_editor,
    );
    optional_field(
        ui,
        "효과",
        &mut rule.effects,
        || global.effects.clone(),
        effects_editor,
    );
    optional_field(
        ui,
        "새 창 지연",
        &mut rule.initialize_delay,
        || global.initialize_delay,
        delay_editor,
    );
    optional_field(
        ui,
        "복원 지연",
        &mut rule.unminimize_delay,
        || global.unminimize_delay,
        delay_editor,
    );
}
