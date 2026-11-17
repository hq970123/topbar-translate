//! Selection translation: grab the text selected in the foreground app and translate it.
//!
//! The grab simulates the system copy shortcut (Ctrl+C / ⌘C) and reads the clipboard,
//! then puts the previous clipboard text back. A marker is written first so an unchanged
//! clipboard (nothing selected) is detected instead of translating stale contents.

use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

const MAX_CHARS: usize = 5000;

pub fn translate_selection(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let res = grab(&app);
        crate::panel::show(&app, true, None);
        match res {
            Ok(Some(text)) => {
                let _ = app.emit("panel:text", text);
            }
            Ok(None) => {
                let _ = app.emit("panel:toast", "没有检测到选中的文字");
            }
            Err(e) => {
                let _ = app.emit("panel:toast", e);
            }
        }
    });
}

fn grab(app: &AppHandle) -> Result<Option<String>, String> {
    platform::ensure_permission()?;
    let cb = app.clipboard();
    let previous = cb.read_text().ok();
    let marker = format!("\u{2063}tt-selection-{}\u{2063}", uuid::Uuid::new_v4());
    cb.write_text(marker.clone()).map_err(|e| format!("无法访问剪贴板：{e}"))?;

    let copy = platform::send_copy();
    let mut got = None;
    if copy.is_ok() {
        // Apps answer the copy command asynchronously; give them up to ~0.6 s.
        for _ in 0..30 {
            thread::sleep(Duration::from_millis(20));
            if let Ok(t) = cb.read_text() {
                if t != marker {
                    got = Some(t);
                    break;
                }
            }
        }
    }

    // Put the user's clipboard back.
    match previous {
        Some(p) => {
            let _ = cb.write_text(p);
        }
        None => {
            let _ = cb.clear();
        }
    }
    copy?;

    Ok(got.and_then(|t| {
        let t = t.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.chars().take(MAX_CHARS).collect())
        }
    }))
}

#[cfg(windows)]
mod platform {
    use std::thread;
    use std::time::{Duration, Instant};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    pub fn ensure_permission() -> Result<(), String> {
        Ok(())
    }

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn down(vk: VIRTUAL_KEY) -> bool {
        unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
    }

    fn send(inputs: &[INPUT]) -> u32 {
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) }
    }

    pub fn send_copy() -> Result<(), String> {
        // The shortcut's own modifiers (e.g. Alt in Ctrl+Alt+D) would turn Ctrl+C into
        // something else. Wait briefly for the user to let go, then force them up.
        let mods = [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN, VK_CONTROL];
        let start = Instant::now();
        while mods.iter().any(|m| down(*m)) && start.elapsed() < Duration::from_millis(400) {
            thread::sleep(Duration::from_millis(10));
        }
        let held: Vec<INPUT> = mods.iter().filter(|m| down(**m)).map(|m| key(*m, true)).collect();
        if !held.is_empty() {
            send(&held);
        }
        let seq = [key(VK_CONTROL, false), key(VK_C, false), key(VK_C, true), key(VK_CONTROL, true)];
        if send(&seq) as usize != seq.len() {
            return Err("无法模拟复制操作（可能被系统或安全软件拦截）".into());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use std::thread;
    use std::time::Duration;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    /// Posting key events needs the Accessibility permission; ask macOS to prompt for it.
    pub fn ensure_permission() -> Result<(), String> {
        let trusted = unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
        };
        if trusted {
            Ok(())
        } else {
            Err("划词翻译需要辅助功能权限：请在 系统设置 → 隐私与安全性 → 辅助功能 中允许 TopBar Translate，然后再试一次".into())
        }
    }

    pub fn send_copy() -> Result<(), String> {
        const KEY_C: u16 = 8; // kVK_ANSI_C
        thread::sleep(Duration::from_millis(60));
        let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| "无法模拟复制操作".to_string())?;
        for is_down in [true, false] {
            let ev = CGEvent::new_keyboard_event(src.clone(), KEY_C, is_down)
                .map_err(|_| "无法模拟复制操作".to_string())?;
            // Explicit flags: only ⌘, whatever the user is still holding.
            ev.set_flags(CGEventFlags::CGEventFlagCommand);
            ev.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn ensure_permission() -> Result<(), String> {
        Ok(())
    }
    pub fn send_copy() -> Result<(), String> {
        Err("当前系统暂不支持划词翻译".into())
    }
}
