//! Persistent settings and translation-provider configuration.
//! API keys are NOT stored here; they live in the OS keychain (see `secrets`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub const INDEX_ID: &str = "index";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub hover_enabled: bool,
    pub auto_hide: bool,
    pub hover_delay_ms: u64,
    pub hide_delay_ms: u64,
    pub trigger_width: f64,
    pub trigger_height: f64,
    pub panel_width: f64,
    pub auto_translate: bool,
    pub auto_translate_delay_ms: u64,
    pub target_lang: String,
    pub second_target_lang: String,
    pub default_provider: String,
    pub shortcut: String,
    /// Global shortcut for screenshot translation.
    pub snip_shortcut: String,
    /// Global shortcut for translating the text currently selected in any app.
    pub select_shortcut: String,
    /// Global shortcut: translate the selected text and paste the translation over it.
    pub replace_shortcut: String,
    /// When the chosen service fails, try the other configured services in order.
    pub failover: bool,
    /// Extra services whose translations are shown under the main result (ids, at most 3).
    pub compare_providers: Vec<String>,
    /// Show phonetics and definitions when the input is a single English word.
    pub word_lookup: bool,
    /// Images: "auto" = send to a vision model when the service supports it, otherwise
    /// recognize text on-device; "local" = always recognize on-device (image never uploaded).
    pub ocr_mode: String,
    /// Whisper model for voice input: tiny | base | small
    pub voice_model: String,
    /// Clear input and result after the panel collapses.
    pub clear_on_hide: bool,
    /// Keep a local translation history (stored in the webview's localStorage).
    pub history_enabled: bool,
    /// Keep at most this many entries (0 = unlimited).
    pub history_max_items: u64,
    /// Drop entries older than this many days (0 = keep forever).
    pub history_max_days: u64,
    pub theme: String,
    /// auto | zh-CN | zh-TW | en | fr | es | ru | ar | ja | ko
    pub ui_language: String,
    pub launch_at_login: bool,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hover_enabled: true,
            auto_hide: true,
            hover_delay_ms: 200,
            hide_delay_ms: 600,
            trigger_width: 200.0,
            trigger_height: 6.0,
            panel_width: 680.0,
            auto_translate: false,
            auto_translate_delay_ms: 800,
            target_lang: "zh-Hans".into(),
            second_target_lang: "en".into(),
            default_provider: INDEX_ID.into(),
            shortcut: "CommandOrControl+Alt+T".into(),
            snip_shortcut: "CommandOrControl+Alt+S".into(),
            select_shortcut: "CommandOrControl+Alt+D".into(),
            replace_shortcut: "CommandOrControl+Alt+R".into(),
            failover: true,
            compare_providers: Vec::new(),
            word_lookup: true,
            ocr_mode: "auto".into(),
            voice_model: "base".into(),
            clear_on_hide: true,
            history_enabled: true,
            history_max_items: 500,
            history_max_days: 30,
            theme: "system".into(),
            ui_language: "auto".into(),
            launch_at_login: false,
            onboarded: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// index | openai | deepseek | gemini | claude | deepl | microsoft | ollama | custom
    pub kind: String,
    pub base_url: String,
    pub model: String,
    /// Azure region for Microsoft Translator.
    pub region: String,
    pub timeout_secs: u64,
    pub enabled: bool,
    pub builtin: bool,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: "custom".into(),
            base_url: String::new(),
            model: String::new(),
            region: String::new(),
            timeout_secs: 30,
            enabled: true,
            builtin: false,
        }
    }
}

pub fn index_provider() -> Provider {
    Provider {
        id: INDEX_ID.into(),
        name: "Index-Translate".into(),
        kind: "index".into(),
        base_url: "https://index-translate.bilibili.com/v1".into(),
        model: "Index-Translate-35B-A3B".into(),
        region: String::new(),
        timeout_secs: 30,
        enabled: true,
        builtin: true,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub settings: Settings,
    pub providers: Vec<Provider>,
}

impl Config {
    pub fn normalize(&mut self) {
        // The built-in source always exists (it can be disabled, never deleted).
        match self.providers.iter_mut().find(|p| p.id == INDEX_ID) {
            Some(p) => {
                let enabled = p.enabled;
                let timeout = p.timeout_secs;
                *p = index_provider();
                p.enabled = enabled;
                p.timeout_secs = timeout.max(5);
            }
            None => self.providers.insert(0, index_provider()),
        }
        let default_ok = self
            .providers
            .iter()
            .any(|p| p.id == self.settings.default_provider && p.enabled);
        if !default_ok {
            self.settings.default_provider = self
                .providers
                .iter()
                .find(|p| p.enabled)
                .map(|p| p.id.clone())
                .unwrap_or_else(|| INDEX_ID.into());
        }
        // Compare list: known services only, no duplicates, at most 3.
        let ids: Vec<String> = self.providers.iter().map(|p| p.id.clone()).collect();
        let mut seen = std::collections::HashSet::new();
        self.settings.compare_providers.retain(|id| ids.contains(id) && seen.insert(id.clone()));
        self.settings.compare_providers.truncate(3);
    }

    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

pub fn load(app: &AppHandle) -> Config {
    let mut cfg = path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .unwrap_or_default();
    cfg.normalize();
    cfg
}

pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let p = path(app).ok_or("无法定位配置目录")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    Ok(())
}

/// Default base URL / model per provider kind, used by the "add API" form.
pub fn kind_defaults(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "openai" => ("https://api.openai.com/v1", "gpt-4o-mini"),
        "deepseek" => ("https://api.deepseek.com/v1", "deepseek-chat"),
        "gemini" => ("https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-flash"),
        "claude" => ("https://api.anthropic.com/v1", "claude-haiku-4-5"),
        "deepl" => ("https://api-free.deepl.com/v2", ""),
        "microsoft" => ("https://api.cognitive.microsofttranslator.com", ""),
        "ollama" => ("http://127.0.0.1:11434/v1", "qwen2.5:7b"),
        _ => ("", ""),
    }
}

pub fn key_required(kind: &str) -> bool {
    !matches!(kind, "index" | "ollama" | "custom")
}

/// Validate a provider base URL so credentials are never sent to an
/// unexpected place: HTTPS is required, plain HTTP only for local / LAN hosts.
pub fn validate_base_url(raw: &str) -> Result<(), String> {
    let u = url::Url::parse(raw.trim()).map_err(|_| "Base URL 格式不正确".to_string())?;
    match u.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = u.host_str().unwrap_or("");
            let local = host == "localhost"
                || host == "127.0.0.1"
                || host == "::1"
                || host == "[::1]"
                || host.starts_with("192.168.")
                || host.starts_with("10.")
                || host.ends_with(".local");
            if local {
                Ok(())
            } else {
                Err("为保护 API Key，非本地地址必须使用 https://".into())
            }
        }
        _ => Err("Base URL 只支持 http(s)".into()),
    }
}

pub mod secrets {
    const SERVICE: &str = "TopBarTranslate";

    pub fn get(id: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, id)
            .ok()?
            .get_password()
            .ok()
            .filter(|s| !s.is_empty())
    }

    pub fn set(id: &str, key: &str) -> Result<(), String> {
        let e = keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())?;
        if key.is_empty() {
            let _ = e.delete_credential();
            Ok(())
        } else {
            e.set_password(key).map_err(|e| format!("无法写入系统安全存储：{e}"))
        }
    }

    pub fn delete(id: &str) {
        if let Ok(e) = keyring::Entry::new(SERVICE, id) {
            let _ = e.delete_credential();
        }
    }
}
