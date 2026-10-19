use crate::imaging::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+A";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Lasso,
    Polygon,
    Rectangle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Lasso,
            hotkey: DEFAULT_HOTKEY.into(),
        }
    }
}

pub fn normalize_hotkey(value: &str) -> Result<String, String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = None;
    for part in value.split('+').map(str::trim) {
        let part = part.to_ascii_uppercase();
        match part.as_str() {
            "CTRL" if !ctrl => ctrl = true,
            "ALT" if !alt => alt = true,
            "SHIFT" if !shift => shift = true,
            _ => {
                let valid = part.len() == 1 && part.as_bytes()[0].is_ascii_alphanumeric()
                    || part
                        .strip_prefix('F')
                        .and_then(|v| v.parse::<u8>().ok())
                        .is_some_and(|n| {
                            (1..=24).contains(&n) && n != 12 && part == format!("F{n}")
                        });
                if key.is_some() || !valid {
                    return Err(
                        "使用 Ctrl / Alt 和字母、数字或 F1–F24（F12 除外），可搭配 Shift。".into(),
                    );
                }
                key = Some(part);
            }
        }
    }
    if !ctrl && !alt || key.is_none() {
        return Err("快捷键需要 Ctrl 或 Alt，以及一个字母、数字或功能键。".into());
    }
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_owned());
    }
    if alt {
        parts.push("Alt".to_owned());
    }
    if shift {
        parts.push("Shift".to_owned());
    }
    parts.push(key.unwrap());
    Ok(parts.join("+"))
}

pub fn load(path: &Path) -> Settings {
    let mut settings = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .unwrap_or_default();
    settings.hotkey = normalize_hotkey(&settings.hotkey).unwrap_or_else(|_| DEFAULT_HOTKEY.into());
    settings
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "无法创建设置目录。")?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| "无法编码应用设置。")?;
    atomic_write(path, &bytes)
}
