mod config;
mod lang;
mod panel;
mod selection;
mod dict;
mod ocr;
mod snip;
mod voice;
mod translate;

use config::{secrets, Config, Provider, Settings};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use translate::ApiError;

pub struct AppState {
    pub cfg: Mutex<Config>,
    pub panel: panel::Shared,
    /// Settings keys whose shortcut another app already holds.
    pub sc_conflicts: Mutex<Vec<String>>,
}

// ---------- view models ----------

#[derive(Serialize)]
struct ProviderView {
    #[serde(flatten)]
    p: Provider,
    has_key: bool,
    key_required: bool,
    supports_image: bool,
}

#[derive(Serialize)]
struct StateView {
    settings: Settings,
    providers: Vec<ProviderView>,
    languages: &'static [lang::Language],
    pinned: bool,
    visible: bool,
    platform: &'static str,
    version: String,
    /// Settings keys whose shortcut could not be registered (taken by another app).
    shortcut_conflicts: Vec<String>,
}

fn view(app: &AppHandle) -> StateView {
    let st = app.state::<AppState>();
    let cfg = st.cfg.lock().unwrap().clone();
    let (pinned, visible) = {
        let p = st.panel.lock().unwrap();
        (p.pinned, p.visible)
    };
    StateView {
        providers: cfg
            .providers
            .iter()
            .map(|p| ProviderView {
                has_key: secrets::get(&p.id).is_some(),
                key_required: config::key_required(&p.kind),
                supports_image: translate::supports_image(&p.kind),
                p: p.clone(),
            })
            .collect(),
        settings: cfg.settings,
        languages: lang::LANGUAGES,
        pinned,
        visible,
        platform: if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" },
        version: app.package_info().version.to_string(),
        shortcut_conflicts: st.sc_conflicts.lock().unwrap().clone(),
    }
}

fn broadcast(app: &AppHandle) {
    let _ = app.emit("config:changed", ());
}

fn persist(app: &AppHandle) -> Result<(), String> {
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    config::save(app, &cfg)
}

// ---------- commands ----------

#[tauri::command]
fn get_state(app: AppHandle) -> StateView {
    view(&app)
}

#[tauri::command]
fn detect_language(text: String) -> String {
    lang::detect(&text)
}

#[derive(Deserialize, Clone)]
pub struct TranslateReq {
    pub text: String,
    pub image: Option<String>,
    pub source: String,
    pub target: String,
    pub provider_id: Option<String>,
    /// Compare rows ask one specific service and must not fall back to another.
    #[serde(default)]
    pub no_failover: bool,
}

#[derive(Serialize)]
pub struct TranslateResp {
    pub text: String,
    source: String,
    target: String,
    provider_id: String,
    provider_name: String,
    ms: u128,
    /// Name of the service that failed before this one answered (automatic failover).
    fallback_from: Option<String>,
    /// Text recognized on-device from the image, when OCR was used.
    ocr_text: Option<String>,
}

/// Errors worth trying another service for. Empty input never is.
fn should_fail_over(e: &ApiError) -> bool {
    !matches!(e.code.as_str(), "empty")
}

#[tauri::command]
async fn translate(app: AppHandle, req: TranslateReq) -> Result<TranslateResp, ApiError> {
    translate_core(&app, req).await
}

pub async fn translate_core(app: &AppHandle, req: TranslateReq) -> Result<TranslateResp, ApiError> {
    let (provider, primary, secondary, backups, ocr_local) = {
        let st = app.state::<AppState>();
        let cfg = st.cfg.lock().unwrap();
        let id = req.provider_id.clone().unwrap_or_else(|| cfg.settings.default_provider.clone());
        let p = cfg
            .provider(&id)
            .cloned()
            .ok_or_else(|| ApiError::new("config", "找不到所选翻译源，请在设置中检查"))?;
        let backups: Vec<config::Provider> = if cfg.settings.failover && !req.no_failover {
            // Default service first, then the rest in the order they are listed.
            let mut v: Vec<config::Provider> = cfg.providers.iter().filter(|x| x.id != p.id).cloned().collect();
            v.sort_by_key(|x| x.id != cfg.settings.default_provider);
            v
        } else {
            Vec::new()
        };
        (p, cfg.settings.target_lang.clone(), cfg.settings.second_target_lang.clone(), backups, cfg.settings.ocr_mode == "local")
    };
    if !provider.enabled {
        return Err(ApiError::new("config", format!("「{}」已被禁用，请切换翻译源", provider.name)));
    }
    let mut text: String = req.text.chars().take(5000).collect();
    let mut image = req.image.clone();
    let mut ocr_text = None;
    // On-device OCR when asked to, or when the service can't read images itself.
    if let Some(img) = image.clone() {
        if ocr_local || !translate::supports_image(&provider.kind) {
            let found = tauri::async_runtime::spawn_blocking(move || ocr::recognize_data_url(&img))
                .await
                .map_err(|e| ApiError::new("ocr", e.to_string()))?
                .map_err(|m| ApiError::new("ocr", m))?;
            let found: String = found.chars().take(5000).collect();
            text = if text.trim().is_empty() { found.clone() } else { format!("{}\n{}", text.trim_end(), found) };
            image = None;
            ocr_text = Some(found);
        }
    }
    let source = if req.source == "auto" || req.source.is_empty() {
        if text.trim().is_empty() { "auto".to_string() } else { lang::detect(&text) }
    } else {
        req.source.clone()
    };
    if req.target != "auto" && lang::find(&req.target).is_none() {
        return Err(ApiError::new("config", "无法识别目标语言，请手动选择"));
    }
    let target = lang::resolve_target(&source, &req.target, &primary, &secondary);
    let started = Instant::now();
    let job = translate::Job {
        text: &text,
        image: image.as_deref(),
        source: &source,
        target: &target,
    };
    let first_err = match translate::run(&provider, secrets::get(&provider.id), &job).await {
        Ok(out) => {
            return Ok(TranslateResp {
                text: out,
                source,
                target,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                ms: started.elapsed().as_millis(),
                fallback_from: None,
                ocr_text,
            })
        }
        Err(e) => e,
    };
    if !should_fail_over(&first_err) {
        return Err(first_err);
    }
    // Automatic failover: at most two more services that are ready to use.
    let mut tried = 0;
    for b in backups {
        if tried >= 2 {
            break;
        }
        let key = secrets::get(&b.id);
        if !b.enabled || (config::key_required(&b.kind) && key.is_none()) {
            continue;
        }
        if job.image.is_some() && !translate::supports_image(&b.kind) {
            continue;
        }
        tried += 1;
        if let Ok(out) = translate::run(&b, key, &job).await {
            return Ok(TranslateResp {
                text: out,
                source,
                target,
                provider_id: b.id.clone(),
                provider_name: b.name.clone(),
                ms: started.elapsed().as_millis(),
                fallback_from: Some(provider.name.clone()),
                ocr_text: ocr_text.clone(),
            });
        }
    }
    Err(first_err)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<StateView, String> {
    let old_shortcut;
    {
        let st = app.state::<AppState>();
        let mut cfg = st.cfg.lock().unwrap();
        old_shortcut = shortcuts_of(&cfg.settings);
        let onboarded = cfg.settings.onboarded;
        cfg.settings = settings;
        cfg.settings.onboarded = onboarded;
        cfg.normalize();
    }
    let (shortcut, autostart) = {
        let st = app.state::<AppState>();
        let cfg = st.cfg.lock().unwrap();
        (shortcuts_of(&cfg.settings), cfg.settings.launch_at_login)
    };
    if shortcut != old_shortcut {
        let new = [&shortcut.0, &shortcut.1, &shortcut.2, &shortcut.3];
        let old = [&old_shortcut.0, &old_shortcut.1, &old_shortcut.2, &old_shortcut.3];
        // Only a shortcut the user just changed is rolled back; one that was already taken stays flagged.
        let bad: Vec<(usize, String)> = apply_shortcut(&app, &shortcut).into_iter().filter(|(i, _)| new[*i] != old[*i]).collect();
        if let Some((_, msg)) = bad.first() {
            let fixed = {
                let st = app.state::<AppState>();
                let mut cfg = st.cfg.lock().unwrap();
                for (i, _) in &bad {
                    let v = old[*i].clone();
                    match i {
                        0 => cfg.settings.shortcut = v,
                        1 => cfg.settings.snip_shortcut = v,
                        2 => cfg.settings.select_shortcut = v,
                        _ => cfg.settings.replace_shortcut = v,
                    }
                }
                shortcuts_of(&cfg.settings)
            };
            apply_shortcut(&app, &fixed);
            persist(&app)?;
            broadcast(&app);
            return Err(msg.clone());
        }
    }
    apply_autostart(&app, autostart);
    persist(&app)?;
    broadcast(&app);
    Ok(view(&app))
}

#[tauri::command]
fn save_provider(app: AppHandle, provider: Provider, api_key: Option<String>, make_default: bool) -> Result<StateView, String> {
    let mut p = provider;
    if p.id == config::INDEX_ID {
        // Built-in: only enable/disable and timeout are editable.
        let st = app.state::<AppState>();
        let mut cfg = st.cfg.lock().unwrap();
        if let Some(cur) = cfg.providers.iter_mut().find(|x| x.id == config::INDEX_ID) {
            cur.enabled = p.enabled;
            cur.timeout_secs = p.timeout_secs.clamp(5, 300);
        }
        if make_default {
            cfg.settings.default_provider = config::INDEX_ID.into();
        }
        cfg.normalize();
    } else {
        p.name = p.name.trim().to_string();
        if p.name.is_empty() {
            return Err("请填写配置名称".into());
        }
        p.base_url = p.base_url.trim().trim_end_matches('/').to_string();
        if p.base_url.is_empty() {
            p.base_url = config::kind_defaults(&p.kind).0.to_string();
        }
        config::validate_base_url(&p.base_url)?;
        p.model = p.model.trim().to_string();
        if !matches!(p.kind.as_str(), "deepl" | "microsoft") && p.model.is_empty() {
            return Err("请填写模型名称".into());
        }
        p.builtin = false;
        p.timeout_secs = p.timeout_secs.clamp(5, 300);
        if p.id.is_empty() {
            p.id = uuid::Uuid::new_v4().to_string();
        }
        if let Some(k) = api_key.as_ref() {
            secrets::set(&p.id, k.trim())?;
        }
        if config::key_required(&p.kind) && secrets::get(&p.id).is_none() {
            return Err("该服务需要填写 API Key".into());
        }
        let st = app.state::<AppState>();
        let mut cfg = st.cfg.lock().unwrap();
        match cfg.providers.iter_mut().find(|x| x.id == p.id) {
            Some(cur) => *cur = p.clone(),
            None => cfg.providers.push(p.clone()),
        }
        if make_default && p.enabled {
            cfg.settings.default_provider = p.id.clone();
        }
        cfg.normalize();
    }
    persist(&app)?;
    broadcast(&app);
    Ok(view(&app))
}

#[tauri::command]
fn delete_provider(app: AppHandle, id: String) -> Result<StateView, String> {
    if id == config::INDEX_ID {
        return Err("内置翻译源不可删除，可以禁用".into());
    }
    {
        let st = app.state::<AppState>();
        let mut cfg = st.cfg.lock().unwrap();
        cfg.providers.retain(|p| p.id != id);
        cfg.normalize();
    }
    secrets::delete(&id);
    persist(&app)?;
    broadcast(&app);
    Ok(view(&app))
}

#[tauri::command]
fn set_default_provider(app: AppHandle, id: String) -> Result<StateView, String> {
    {
        let st = app.state::<AppState>();
        let mut cfg = st.cfg.lock().unwrap();
        match cfg.providers.iter().find(|p| p.id == id) {
            Some(p) if p.enabled => cfg.settings.default_provider = id,
            Some(_) => return Err("该翻译源已禁用".into()),
            None => return Err("找不到该翻译源".into()),
        }
    }
    persist(&app)?;
    broadcast(&app);
    Ok(view(&app))
}

/// Test a provider config (possibly unsaved) with a short sample.
#[tauri::command]
async fn test_provider(provider: Provider, api_key: Option<String>) -> Result<String, ApiError> {
    let mut p = provider;
    if p.id == config::INDEX_ID {
        p = config::index_provider();
    }
    if p.base_url.trim().is_empty() {
        p.base_url = config::kind_defaults(&p.kind).0.to_string();
    }
    let key = match api_key.filter(|k| !k.trim().is_empty()) {
        Some(k) => Some(k.trim().to_string()),
        None if !p.id.is_empty() => secrets::get(&p.id),
        None => None,
    };
    let job = translate::Job { text: "Hello, world!", image: None, source: "en", target: "zh-Hans" };
    translate::run(&p, key, &job).await
}

#[tauri::command]
fn kind_defaults(kind: String) -> (String, String) {
    let (a, b) = config::kind_defaults(&kind);
    (a.into(), b.into())
}

#[tauri::command]
fn set_pinned(app: AppHandle, pinned: bool) {
    app.state::<AppState>().panel.lock().unwrap().pinned = pinned;
    let _ = app.emit("panel:pinned", pinned);
}

/// The UI reports activity so the panel does not collapse under the user.
#[tauri::command]
fn ui_activity(app: AppHandle, typing: bool, hold: bool) {
    let st = app.state::<AppState>();
    let mut s = st.panel.lock().unwrap();
    if typing {
        s.typing_until = Instant::now() + Duration::from_millis(1500);
    }
    s.hold = hold;
}

#[tauri::command]
fn resize_panel(app: AppHandle, height: f64) {
    panel::set_content_height(&app, height);
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    panel::hide(&app);
}

#[tauri::command]
async fn open_settings(app: AppHandle, tab: Option<String>) {
    show_settings(&app, tab);
}

#[tauri::command]
async fn lookup_word(app: AppHandle, word: String) -> Option<dict::Entry> {
    let on = app.state::<AppState>().cfg.lock().unwrap().settings.word_lookup;
    if !on {
        return None;
    }
    dict::lookup(&word).await
}

/// Save the vocabulary list (CSV built by the UI) into the Downloads folder.
#[tauri::command]
fn export_vocab(app: AppHandle, csv: String) -> Result<String, String> {
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .map_err(|e| format!("找不到下载文件夹：{e}"))?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("TopBarTranslate-vocab-{secs}.csv"));
    // UTF-8 BOM so Excel opens Chinese text correctly.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(csv.as_bytes());
    std::fs::write(&path, bytes).map_err(|e| format!("导出失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Only called when the user presses the paste button (never read proactively).
#[tauri::command]
fn paste_text(app: AppHandle) -> Result<String, String> {
    app.clipboard().read_text().map_err(|_| "剪贴板中没有文本".to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

// ---------- helpers ----------

type Shortcuts = (String, String, String, String);

fn shortcuts_of(s: &config::Settings) -> Shortcuts {
    (s.shortcut.clone(), s.snip_shortcut.clone(), s.select_shortcut.clone(), s.replace_shortcut.clone())
}

const SHORTCUT_KEYS: [&str; 4] = ["shortcut", "snip_shortcut", "select_shortcut", "replace_shortcut"];

/// Register every shortcut on its own, so one taken combination doesn't block the rest.
/// Returns (index, message) for each one that failed and remembers them for the settings UI.
fn apply_shortcut(app: &AppHandle, sc: &Shortcuts) -> Vec<(usize, String)> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut failed = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for (i, s) in [sc.0.as_str(), sc.1.as_str(), sc.2.as_str(), sc.3.as_str()].into_iter().enumerate() {
        if s.trim().is_empty() {
            continue;
        }
        let parsed = match s.parse::<tauri_plugin_global_shortcut::Shortcut>() {
            Ok(p) => p,
            Err(e) => {
                failed.push((i, format!("快捷键 {s} 格式无效：{e}")));
                continue;
            }
        };
        if seen.contains(&parsed.id()) {
            failed.push((i, "快捷键不能重复".to_string()));
            continue;
        }
        seen.push(parsed.id());
        if let Err(e) = gs.register(parsed) {
            failed.push((i, format!("快捷键 {s} 无法注册（可能已被占用）：{e}")));
        }
    }
    *app.state::<AppState>().sc_conflicts.lock().unwrap() =
        failed.iter().map(|(i, _)| SHORTCUT_KEYS[*i].to_string()).collect();
    failed
}

fn shortcut_is(combo: &str, sc: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    !combo.trim().is_empty()
        && combo.parse::<tauri_plugin_global_shortcut::Shortcut>().map(|s| s.id() == sc.id()).unwrap_or(false)
}

enum ShortcutAction {
    Toggle,
    Snip,
    Selection,
    Replace,
}

fn shortcut_action(app: &AppHandle, sc: &tauri_plugin_global_shortcut::Shortcut) -> ShortcutAction {
    let (_, snip, sel, rep) = shortcuts_of(&app.state::<AppState>().cfg.lock().unwrap().settings);
    if shortcut_is(&snip, sc) {
        ShortcutAction::Snip
    } else if shortcut_is(&sel, sc) {
        ShortcutAction::Selection
    } else if shortcut_is(&rep, sc) {
        ShortcutAction::Replace
    } else {
        ShortcutAction::Toggle
    }
}

#[tauri::command]
fn start_snip(app: AppHandle) {
    snip::start(&app);
}

/// Running from an MSIX package (Microsoft Store)? Registry Run keys don't apply there.
#[cfg(windows)]
fn msix_packaged() -> bool {
    windows::ApplicationModel::Package::Current().is_ok()
}

/// Packaged launch-at-login: the StartupTask declared in AppxManifest.xml.
#[cfg(windows)]
fn apply_startup_task(on: bool) -> windows::core::Result<()> {
    use windows::ApplicationModel::{StartupTask, StartupTaskState};
    let task = StartupTask::GetAsync(&windows::core::HSTRING::from("TopBarTranslateStartup"))?.get()?;
    let state = task.State()?;
    if on {
        if state == StartupTaskState::Disabled {
            task.RequestEnableAsync()?.get()?;
        }
    } else if state == StartupTaskState::Enabled {
        task.Disable()?;
    }
    Ok(())
}

fn apply_autostart(app: &AppHandle, on: bool) {
    #[cfg(windows)]
    if msix_packaged() {
        let _ = apply_startup_task(on);
        return;
    }
    let al = app.autolaunch();
    let cur = al.is_enabled().unwrap_or(false);
    if on && !cur {
        let _ = al.enable();
    } else if !on && cur {
        let _ = al.disable();
    }
}

fn show_settings(app: &AppHandle, tab: Option<String>) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(t) = tab {
            let _ = w.emit("settings:tab", t);
        }
        return;
    }
    let url = match tab {
        Some(t) => format!("settings.html#{t}"),
        None => "settings.html".to_string(),
    };
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App(url.into()))
        .title(ui_strings(app).get(4).cloned().unwrap_or_else(|| "TopBar Translate".into()))
        .inner_size(820.0, 600.0)
        .min_inner_size(700.0, 500.0)
        .center()
        .focused(true)
        .build();
}

/// Localized native strings pushed by the UI: [show, settings, apis, quit, settings title].
struct UiStrings {
    texts: Mutex<Vec<String>>,
    items: Mutex<Vec<MenuItem<tauri::Wry>>>,
}

fn ui_strings(app: &AppHandle) -> Vec<String> {
    app.try_state::<UiStrings>().map(|s| s.texts.lock().unwrap().clone()).unwrap_or_default()
}

#[tauri::command]
fn set_ui_strings(app: AppHandle, strings: Vec<String>) {
    if let Some(st) = app.try_state::<UiStrings>() {
        for (item, text) in st.items.lock().unwrap().iter().zip(strings.iter()) {
            let _ = item.set_text(text);
        }
        if let (Some(w), Some(t)) = (app.get_webview_window("settings"), strings.get(4)) {
            let _ = w.set_title(t);
        }
        *st.texts.lock().unwrap() = strings;
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示翻译窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let apis = MenuItem::with_id(app, "apis", "API 管理…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 TopBar Translate", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings, &apis, &sep, &quit])?;
    app.manage(UiStrings {
        texts: Mutex::new(Vec::new()),
        items: Mutex::new(vec![show.clone(), settings.clone(), apis.clone(), quit.clone()]),
    });

    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = app.default_window_icon().cloned().expect("app icon");

    let builder = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("TopBar Translate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "show" => panel::show(app, true, None),
            "settings" => show_settings(app, None),
            "apis" => show_settings(app, Some("api".into())),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                panel::toggle(tray.app_handle());
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder.build(app)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_window_tweaks(win: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    if let Ok(ns) = win.ns_window() {
        unsafe {
            let obj: &AnyObject = &*(ns as *mut AnyObject);
            // CanJoinAllSpaces | Stationary | IgnoresCycle | FullScreenAuxiliary:
            // visible on every Space and above full-screen apps.
            let behavior: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
            let _: () = msg_send![obj, setCollectionBehavior: behavior];
            // NSStatusWindowLevel, above the menu bar.
            let _: () = msg_send![obj, setLevel: 25isize];
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            panel::show(app, true, None);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        match shortcut_action(app, shortcut) {
                            ShortcutAction::Snip => snip::start(app),
                            ShortcutAction::Selection => selection::translate_selection(app),
                            ShortcutAction::Replace => selection::replace_selection(app),
                            ShortcutAction::Toggle => panel::toggle(app),
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let cfg = config::load(&handle);
            let shortcut = shortcuts_of(&cfg.settings);
            let first_run = !cfg.settings.onboarded;
            app.manage(AppState {
                cfg: Mutex::new(cfg),
                panel: Arc::new(Mutex::new(panel::PanelState::default())),
                sc_conflicts: Mutex::new(Vec::new()),
            });

            app.manage(snip::SnipState::default());
            app.manage(voice::VoiceState::default());
            for (_, e) in apply_shortcut(&handle, &shortcut) {
                eprintln!("{e}");
            }
            // Keep the OS login item in sync with the setting.
            let autostart = app.state::<AppState>().cfg.lock().unwrap().settings.launch_at_login;
            apply_autostart(&handle, autostart);

            build_tray(&handle)?;

            #[cfg(target_os = "macos")]
            if let Some(w) = app.get_webview_window("main") {
                macos_window_tweaks(&w);
            }

            panel::start_watcher(handle.clone());

            if first_run {
                {
                    let st = app.state::<AppState>();
                    st.cfg.lock().unwrap().settings.onboarded = true;
                }
                let _ = persist(&handle);
                // Show once so a new user sees where the panel lives.
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(800));
                    panel::show(&h, true, None);
                });
            }
            Ok(())
        })
        .on_window_event(|w, e| {
            if w.label() != "main" {
                return;
            }
            match e {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    panel::hide(w.app_handle());
                }
                WindowEvent::Focused(false) => {
                    // A panel opened from the shortcut/tray closes when it loses focus.
                    let app = w.app_handle().clone();
                    let (sticky, pinned, visible) = {
                        let s = app.state::<AppState>();
                        let s = s.panel.lock().unwrap();
                        (s.sticky, s.pinned, s.visible)
                    };
                    if visible && sticky && !pinned {
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(150));
                            let refocused = app
                                .get_webview_window("main")
                                .and_then(|w| w.is_focused().ok())
                                .unwrap_or(false);
                            if !refocused {
                                panel::hide(&app);
                            }
                        });
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            detect_language,
            translate,
            save_settings,
            set_ui_strings,
            start_snip,
            snip::snip_image,
            snip::snip_ready,
            snip::snip_finish,
            voice::voice_status,
            voice::voice_download,
            voice::voice_delete,
            voice::voice_cancel_download,
            voice::voice_start,
            voice::voice_stop,
            save_provider,
            delete_provider,
            set_default_provider,
            test_provider,
            kind_defaults,
            set_pinned,
            ui_activity,
            resize_panel,
            hide_panel,
            open_settings,
            copy_text,
            lookup_word,
            export_vocab,
            paste_text,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TopBar Translate")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // Keep running in the tray when windows close; only explicit quit exits.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
