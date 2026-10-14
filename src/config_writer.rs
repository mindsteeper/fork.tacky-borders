//! config.yaml writer used by the settings GUI
//!   - serde_yaml_ng cannot emit comments, so the file is assembled key by key here with a
//!     Korean one-liner above each top-level and global entry to keep it self-documenting

use anyhow::Context;
use serde_yaml_ng::Value;
use std::fmt::Write;

use crate::config::Config;

const HEADER: &str = "\
# tacky-borders 설정 파일
#   - 트레이 아이콘 > 설정... 에서 저장하면 이 파일이 다시 쓰여짐 (기존 주석은 유지되지 않음)
#   - 직접 편집해도 됨. 전체 항목 설명: https://github.com/luke-you/tacky-borders#configuration-options
";

/// Korean description for each top-level / global key. Keys without one get no comment.
fn comment_for(key: &str) -> Option<&'static str> {
    Some(match key {
        "watch_config_changes" => "설정 파일이 바뀌면 자동으로 테두리를 다시 불러옴",
        "enable_logging" => "로그를 파일(tacky-borders.log)에 기록. 변경 시 프로그램 재시작 필요",
        "enable_ipc_server" => "CLI(tacky-borders.exe <명령>)로 보내는 IPC 명령을 처리",
        "rendering_backend" => "렌더러 종류: V2(효과 지원, 권장) 또는 Legacy(단순)",
        "global" => "모든 창에 적용되는 기본 설정",
        "window_rules" => {
            "창별 규칙. 제목(Title)/클래스(Class)/프로세스(Process) 이름으로 창을 찾아 설정을 덮어씀"
        }
        "border_width" => "테두리 두께 (px)",
        "border_offset" => {
            "창 가장자리 기준 테두리 위치 (px). 음수면 안쪽, 양수면 바깥쪽. 숫자 하나 또는 top/left/right/bottom"
        }
        "border_radius" => "모서리 반지름: Auto, Square, Round, RoundSmall 또는 숫자",
        "border_z_order" => "테두리 Z 순서: AboveWindow(창 위) 또는 BelowWindow(창 아래)",
        "follow_native_border" => "Windows 기본 테두리의 표시 여부를 따라감",
        "active_color" => {
            "활성(포커스) 창 테두리 색. 16진수 색상, accent, 그라데이션(colors/direction), 테마별(dark/light)"
        }
        "inactive_color" => "비활성 창 테두리 색",
        "komorebi_colors" => "komorebi 창 종류별 색 (stack/monocle/floating)",
        "animations" => "테두리 애니메이션 (Spiral, ReverseSpiral, Fade)",
        "effects" => "테두리 효과 (Shadow, Glow). rendering_backend: V2 필요",
        "initialize_delay" => "새 창이 열린 뒤 테두리가 나타나기까지의 지연 (ms)",
        "unminimize_delay" => "최소화 해제 뒤 테두리가 나타나기까지의 지연 (ms)",
        _ => return None,
    })
}

pub fn to_commented_yaml(config: &Config) -> anyhow::Result<String> {
    let value = serde_yaml_ng::to_value(config).context("could not serialize config")?;
    let Value::Mapping(root) = value else {
        anyhow::bail!("config did not serialize to a mapping");
    };

    let mut out = String::from(HEADER);

    for (key, value) in &root {
        let key = key.as_str().context("config key is not a string")?;

        out.push('\n');
        if let Some(comment) = comment_for(key) {
            writeln!(out, "# {comment}")?;
        }

        match (key, value) {
            ("global", Value::Mapping(global)) => {
                writeln!(out, "{key}:")?;

                for (i, (global_key, global_value)) in global.iter().enumerate() {
                    let global_key = global_key
                        .as_str()
                        .context("global config key is not a string")?;

                    if i > 0 {
                        out.push('\n');
                    }
                    if let Some(comment) = comment_for(global_key) {
                        writeln!(out, "  # {comment}")?;
                    }
                    write_entry(&mut out, global_key, global_value, 2)?;
                }
            }
            _ => write_entry(&mut out, key, value, 0)?,
        }
    }

    Ok(out)
}

/// Writes `key: value`, putting non-empty mappings/sequences on the following lines
fn write_entry(out: &mut String, key: &str, value: &Value, indent: usize) -> anyhow::Result<()> {
    let pad = " ".repeat(indent);
    let is_block = match value {
        Value::Mapping(mapping) => !mapping.is_empty(),
        Value::Sequence(sequence) => !sequence.is_empty(),
        _ => false,
    };

    let text = serde_yaml_ng::to_string(value).context("could not serialize config value")?;

    if is_block {
        writeln!(out, "{pad}{key}:")?;
        for line in text.lines() {
            writeln!(out, "{pad}  {line}")?;
        }
    } else {
        writeln!(out, "{pad}{key}: {}", text.trim_end())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animations::{AnimEasing, AnimParamsConfig, AnimType, AnimationsConfig};
    use crate::colors::{
        ColorBrushConfig, GradientBrushConfig, GradientDirection, ThemeAwareColor,
    };
    use crate::config::{
        EnableMode, MatchKind, MatchStrategy, Offset, OffsetConfig, RadiusConfig, WidthConfig,
        WindowRule, ZOrderMode,
    };
    use crate::effects::{EffectParamsConfig, EffectType, EffectsConfig, Translation};

    fn round_trip(config: &Config) -> Config {
        let yaml = to_commented_yaml(config).expect("could not write yaml");
        serde_yaml_ng::from_str(&yaml).unwrap_or_else(|err| panic!("{err}\n---\n{yaml}"))
    }

    #[test]
    fn default_config_round_trips() {
        let config = Config::from_default_yaml().expect("bundled config should parse");
        assert_eq!(round_trip(&config), config);
    }

    #[test]
    fn complex_config_round_trips() {
        let mut config = Config::from_default_yaml().unwrap();
        config.render_backend = crate::render_backend::RenderBackendConfig::Legacy;
        config.global.border_width = WidthConfig::new(2.5);
        config.global.border_offset = OffsetConfig::PerSide(Offset {
            top: -1,
            left: 0,
            right: 2,
            bottom: -3,
        });
        config.global.border_radius = RadiusConfig::Custom(12.0);
        config.global.border_z_order = ZOrderMode::BelowWindow;
        config.global.active_color = ColorBrushConfig::ThemeAware(ThemeAwareColor {
            dark: Box::new(ColorBrushConfig::Solid("#ffffff80".into())),
            light: Box::new(ColorBrushConfig::Gradient(GradientBrushConfig {
                colors: vec!["#000000".into(), "accent".into()],
                direction: GradientDirection::Angle("90deg".into()),
            })),
        });
        config.global.animations = AnimationsConfig {
            active: vec![AnimParamsConfig {
                anim_type: AnimType::Fade,
                duration: Some(150.0),
                easing: Some(AnimEasing::CubicBezier([0.1, 0.2, 0.3, 0.4])),
            }],
            inactive: vec![],
            fps: 120,
            enabled: true,
        };
        config.global.effects = EffectsConfig {
            active: vec![EffectParamsConfig {
                effect_type: EffectType::Shadow,
                std_dev: 4.0,
                spread: 1.0,
                opacity: 0.5,
                translation: Translation { x: 1.0, y: 2.0 },
            }],
            inactive: vec![],
            enabled: false,
        };
        config.window_rules = vec![
            WindowRule {
                kind: Some(MatchKind::Process),
                name: Some("explorer".into()),
                strategy: Some(MatchStrategy::Contains),
                border_width: Some(WidthConfig::new(1.0)),
                enabled: Some(EnableMode::Bool(true)),
                ..Default::default()
            },
            WindowRule {
                kind: Some(MatchKind::Title),
                name: Some("한글 제목: #특수".into()),
                enabled: Some(EnableMode::Auto),
                ..Default::default()
            },
        ];

        assert_eq!(round_trip(&config), config);
    }

    #[test]
    fn empty_rules_are_written_inline() {
        let mut config = Config::from_default_yaml().unwrap();
        config.window_rules.clear();

        let yaml = to_commented_yaml(&config).unwrap();
        assert!(yaml.contains("window_rules: []"), "{yaml}");
        assert_eq!(round_trip(&config), config);
    }
}
