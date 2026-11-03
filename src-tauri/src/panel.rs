//! The top-of-screen panel: hover trigger, positioning, auto-hide.

use crate::AppState;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition};

/// Transparent margin (logical px) around the visible card, for its shadow.
pub const MARGIN: f64 = 0.0;

pub struct PanelState {
    pub visible: bool,
    pub pinned: bool,
    /// Opened from the shortcut / tray: mouse leaving does not hide it.
    pub sticky: bool,
    /// The UI asked to stay open (dropdown open, modal, IME composition...).
    pub hold: bool,
    /// The user typed recently: do not hide before this instant.
    pub typing_until: Instant,
    pub gen: u64,
    /// Card height reported by the UI, logical px (without margins).
    pub content_height: f64,
    /// Physical rect of the window while visible: x, y, w, h.
    pub rect: Option<(f64, f64, f64, f64)>,
    pub scale: f64,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            visible: false,
            pinned: false,
            sticky: false,
            hold: false,
            typing_until: Instant::now(),
            gen: 0,
            content_height: 340.0,
            rect: None,
            scale: 1.0,
        }
    }
}

pub type Shared = Arc<Mutex<PanelState>>;

fn shared(app: &AppHandle) -> Shared {
    app.state::<AppState>().panel.clone()
}

fn contains(m: &Monitor, p: &PhysicalPosition<f64>) -> bool {
    let pos = m.position();
    let size = m.size();
    p.x >= pos.x as f64
        && p.x < pos.x as f64 + size.width as f64
        && p.y >= pos.y as f64
        && p.y < pos.y as f64 + size.height as f64
}

pub fn monitor_at(app: &AppHandle, p: Option<PhysicalPosition<f64>>) -> Option<Monitor> {
    if let Some(p) = p {
        if let Ok(list) = app.available_monitors() {
            if let Some(m) = list.into_iter().find(|m| contains(m, &p)) {
                return Some(m);
            }
        }
    }
    app.primary_monitor().ok().flatten()
}

fn window_logical_size(app: &AppHandle, st: &PanelState) -> (f64, f64) {
    let width = app.state::<AppState>().cfg.lock().unwrap().settings.panel_width.clamp(420.0, 1200.0);
    (width + MARGIN * 2.0, st.content_height.clamp(120.0, 900.0) + MARGIN * 2.0)
}

/// Show the panel at the top centre of the monitor under the cursor.
pub fn show(app: &AppHandle, sticky: bool, near: Option<PhysicalPosition<f64>>) {
    let Some(win) = app.get_webview_window("main") else { return };
    let near = near.or_else(|| app.cursor_position().ok());
    let mon = monitor_at(app, near);
    let st = shared(app);
    {
        let mut s = st.lock().unwrap();
        let (w, h) = window_logical_size(app, &s);
        if let Some(m) = mon {
            let scale = m.scale_factor();
            let work = m.work_area();
            let w_phys = (w * scale).round();
            let h_phys = (h * scale).round();
            let x = m.position().x as f64 + (m.size().width as f64 - w_phys) / 2.0;
            // Work area excludes the macOS menu bar / notch and a top taskbar on Windows.
            let y = work.position.y as f64;
            let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            let _ = win.set_size(LogicalSize::new(w, h));
            s.rect = Some((x, y, w_phys, h_phys));
            s.scale = scale;
        } else {
            let _ = win.set_size(LogicalSize::new(w, h));
        }
        s.visible = true;
        s.sticky = sticky;
        s.gen += 1;
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.emit("panel:show", sticky);
}

pub fn hide(app: &AppHandle) {
    let st = shared(app);
    let gen = {
        let mut s = st.lock().unwrap();
        if !s.visible {
            return;
        }
        s.visible = false;
        s.sticky = false;
        s.hold = false;
        s.gen += 1;
        s.gen
    };
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.emit("panel:hide", ());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // Let the 150ms collapse animation finish.
        std::thread::sleep(Duration::from_millis(160));
        let still_hidden = {
            let s = shared(&app);
            let s = s.lock().unwrap();
            s.gen == gen && !s.visible
        };
        if still_hidden {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }
            #[cfg(target_os = "macos")]
            {
                // Hand focus back to the previously active app, unless settings are open.
                let settings_open = app
                    .get_webview_window("settings")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                if !settings_open {
                    let _ = app.hide();
                }
            }
        }
    });
}

pub fn toggle(app: &AppHandle) {
    let visible = shared(app).lock().unwrap().visible;
    if visible {
        hide(app)
    } else {
        show(app, true, None)
    }
}

/// Resize the window to fit the card (called by the UI's ResizeObserver).
pub fn set_content_height(app: &AppHandle, height: f64) {
    let st = shared(app);
    let mut s = st.lock().unwrap();
    if (s.content_height - height).abs() < 0.5 {
        return;
    }
    s.content_height = height;
    let (w, h) = window_logical_size(app, &s);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_size(LogicalSize::new(w, h));
    }
    let scale = s.scale;
    if let Some(r) = s.rect.as_mut() {
        r.3 = (h * scale).round();
    }
}

struct HoverCfg {
    enabled: bool,
    auto_hide: bool,
    hover_delay: Duration,
    hide_delay: Duration,
    tw: f64,
    th: f64,
}

/// Background loop: poll the cursor, open on dwell in the trigger strip,
/// close after the cursor has been away long enough.
pub fn start_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut enter_at: Option<Instant> = None;
        let mut last_inside = Instant::now();
        let mut monitors: Vec<Monitor> = Vec::new();
        let mut monitors_at = Instant::now() - Duration::from_secs(60);
        loop {
            std::thread::sleep(Duration::from_millis(35));
            let cfg = {
                let state = app.state::<AppState>();
                let guard = state.cfg.lock().unwrap();
                let s = &guard.settings;
                HoverCfg {
                    enabled: s.hover_enabled,
                    auto_hide: s.auto_hide,
                    hover_delay: Duration::from_millis(s.hover_delay_ms.min(3000)),
                    hide_delay: Duration::from_millis(s.hide_delay_ms.clamp(100, 10000)),
                    tw: s.trigger_width.clamp(20.0, 4000.0),
                    th: s.trigger_height.clamp(1.0, 60.0),
                }
            };
            let Ok(p) = app.cursor_position() else { continue };
            if monitors_at.elapsed() > Duration::from_secs(3) {
                monitors = app.available_monitors().unwrap_or_default();
                monitors_at = Instant::now();
            }
            let in_trigger = monitors.iter().any(|m| {
                let s = m.scale_factor();
                let (tw, th) = (cfg.tw * s, cfg.th * s);
                let mx = m.position().x as f64;
                let my = m.position().y as f64;
                let cx = mx + m.size().width as f64 / 2.0;
                p.x >= cx - tw / 2.0 && p.x <= cx + tw / 2.0 && p.y >= my && p.y < my + th
            });
            let st = shared(&app);
            let (visible, pinned, sticky, hold, typing_until, rect) = {
                let s = st.lock().unwrap();
                (s.visible, s.pinned, s.sticky, s.hold, s.typing_until, s.rect)
            };
            let now = Instant::now();
            if !visible {
                if cfg.enabled && in_trigger {
                    let t = *enter_at.get_or_insert(now);
                    if now.duration_since(t) >= cfg.hover_delay {
                        enter_at = None;
                        last_inside = now;
                        show(&app, false, Some(p));
                    }
                } else {
                    enter_at = None;
                }
                continue;
            }
            enter_at = None;
            let pad = 6.0;
            let inside = in_trigger
                || rect
                    .map(|(x, y, w, h)| p.x >= x - pad && p.x <= x + w + pad && p.y >= y - pad - 40.0 && p.y <= y + h + pad)
                    .unwrap_or(true);
            if inside {
                last_inside = now;
                continue;
            }
            if cfg.auto_hide
                && !pinned
                && !sticky
                && !hold
                && now >= typing_until
                && now.duration_since(last_inside) >= cfg.hide_delay
            {
                hide(&app);
            }
        }
    });
}
