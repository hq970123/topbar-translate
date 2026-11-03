//! Screenshot translation: capture the monitor under the cursor, let the user drag a
//! region on a full-screen overlay, crop it and hand the image to the panel.
use base64::Engine;
use std::io::Cursor;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use xcap::image::{codecs::png::{CompressionType, FilterType, PngEncoder}, ImageEncoder, RgbaImage};

#[derive(Default)]
pub struct SnipState(Mutex<Option<RgbaImage>>);

fn png_data_url(img: &RgbaImage, fast: bool) -> Result<String, String> {
    let mut buf = Vec::new();
    let enc = if fast {
        PngEncoder::new_with_quality(Cursor::new(&mut buf), CompressionType::Fast, FilterType::NoFilter)
    } else {
        PngEncoder::new(Cursor::new(&mut buf))
    };
    enc.write_image(img.as_raw(), img.width(), img.height(), xcap::image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf)))
}

/// Start a screenshot: hide the panel, capture, open the selection overlay.
pub fn start(app: &AppHandle) {
    if app.get_webview_window("snip").is_some() {
        return;
    }
    crate::panel::hide(app);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(220)); // let the panel slide away
        let cursor = app.cursor_position().ok();
        let Some(mon) = crate::panel::monitor_at(&app, cursor) else { return };
        let (pos, size, scale) = (*mon.position(), *mon.size(), mon.scale_factor());
        // xcap uses physical pixels on Windows and points on macOS.
        let (cx, cy) = match cursor {
            Some(p) => (p.x, p.y),
            None => (pos.x as f64 + 1.0, pos.y as f64 + 1.0),
        };
        let (qx, qy) = if cfg!(target_os = "macos") { (cx / scale, cy / scale) } else { (cx, cy) };
        let shot = xcap::Monitor::from_point(qx as i32, qy as i32)
            .or_else(|_| xcap::Monitor::all().and_then(|v| v.into_iter().next().ok_or(xcap::XCapError::new("no monitor"))))
            .and_then(|m| m.capture_image());
        let img = match shot {
            Ok(i) => i,
            Err(e) => {
                let _ = app.emit("panel:toast", format!("截图失败：{e}"));
                crate::panel::show(&app, true, cursor);
                return;
            }
        };
        *app.state::<SnipState>().0.lock().unwrap() = Some(img);
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let built = WebviewWindowBuilder::new(&a, "snip", WebviewUrl::App("snip.html".into()))
                .title("TopBar Translate")
                .decorations(false)
                .resizable(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .shadow(false)
                .visible(false)
                .build();
            if let Ok(w) = built {
                let _ = w.set_position(PhysicalPosition::new(pos.x, pos.y));
                let _ = w.set_size(PhysicalSize::new(size.width, size.height));
            }
        });
    });
}

/// Overlay asks for the captured screen as a data URL.
#[tauri::command]
pub fn snip_image(app: AppHandle) -> Result<String, String> {
    let st = app.state::<SnipState>();
    let g = st.0.lock().unwrap();
    let img = g.as_ref().ok_or("no screenshot")?;
    png_data_url(img, true)
}

#[tauri::command]
pub fn snip_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("snip") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Finish: rect is in fractions (0..1) of the overlay; None = cancelled.
#[tauri::command]
pub fn snip_finish(app: AppHandle, rect: Option<[f64; 4]>) {
    if let Some(w) = app.get_webview_window("snip") {
        let _ = w.close();
    }
    let img = app.state::<SnipState>().0.lock().unwrap().take();
    let (Some(img), Some([x, y, w, h])) = (img, rect) else { return };
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let px = (x.clamp(0.0, 1.0) * iw).round() as u32;
    let py = (y.clamp(0.0, 1.0) * ih).round() as u32;
    let pw = ((w.clamp(0.0, 1.0) * iw).round() as u32).min(img.width() - px.min(img.width()));
    let ph = ((h.clamp(0.0, 1.0) * ih).round() as u32).min(img.height() - py.min(img.height()));
    if pw < 4 || ph < 4 {
        return;
    }
    let crop = xcap::image::imageops::crop_imm(&img, px, py, pw, ph).to_image();
    match png_data_url(&crop, false) {
        Ok(url) => {
            crate::panel::show(&app, true, None);
            let _ = app.emit("panel:image", url);
        }
        Err(e) => {
            let _ = app.emit("panel:toast", format!("截图失败：{e}"));
        }
    }
}
