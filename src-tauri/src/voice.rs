//! Offline voice input: record from the default microphone with cpal and transcribe
//! locally with whisper.cpp. Models are not bundled; the user downloads one in Settings.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// (name, approximate size in MB)
pub const MODELS: &[(&str, u32)] = &[("tiny", 75), ("base", 142), ("small", 466)];
const SOURCES: &[&str] = &[
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main",
    "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main",
];

struct Recording {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    thread: std::thread::JoinHandle<()>,
}

#[derive(Default)]
pub struct VoiceState {
    rec: Mutex<Option<Recording>>,
    ctx: Mutex<Option<(String, Arc<WhisperContext>)>>,
    downloading: Mutex<Option<String>>,
    cancel: AtomicBool,
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app.path().app_data_dir().map_err(|e| e.to_string())?.join("models");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn model_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    if !MODELS.iter().any(|(n, _)| *n == name) {
        return Err("未知的语音模型".into());
    }
    Ok(models_dir(app)?.join(format!("ggml-{name}.bin")))
}

fn selected_model(app: &AppHandle) -> String {
    app.state::<crate::AppState>().cfg.lock().unwrap().settings.voice_model.clone()
}

#[derive(Serialize)]
pub struct ModelInfo {
    name: String,
    size_mb: u32,
    downloaded: bool,
}

#[derive(Serialize)]
pub struct VoiceStatus {
    models: Vec<ModelInfo>,
    selected: String,
    /// The selected model is on disk, so voice input can be shown.
    ready: bool,
    downloading: Option<String>,
}

#[tauri::command]
pub fn voice_status(app: AppHandle) -> Result<VoiceStatus, String> {
    let models: Vec<ModelInfo> = MODELS
        .iter()
        .map(|(n, mb)| ModelInfo {
            name: n.to_string(),
            size_mb: *mb,
            downloaded: model_path(&app, n).map(|p| p.exists()).unwrap_or(false),
        })
        .collect();
    let selected = selected_model(&app);
    let ready = models.iter().any(|m| m.name == selected && m.downloaded);
    let downloading = app.state::<VoiceState>().downloading.lock().unwrap().clone();
    Ok(VoiceStatus { models, selected, ready, downloading })
}

#[derive(Serialize, Clone)]
struct Progress {
    name: String,
    received: u64,
    total: u64,
}

/// Download a model with progress events (`voice:progress`); tries the mirror if Hugging Face fails.
#[tauri::command]
pub async fn voice_download(app: AppHandle, name: String) -> Result<(), String> {
    let path = model_path(&app, &name)?;
    {
        let st = app.state::<VoiceState>();
        let mut d = st.downloading.lock().unwrap();
        if d.is_some() {
            return Err("已有模型正在下载".into());
        }
        *d = Some(name.clone());
        st.cancel.store(false, Ordering::SeqCst);
    }
    let res = download(&app, &name, &path).await;
    *app.state::<VoiceState>().downloading.lock().unwrap() = None;
    let _ = app.emit("voice:changed", ());
    res
}

async fn download(app: &AppHandle, name: &str, path: &PathBuf) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let part = path.with_extension("part");
    let mut last_err = String::from("网络请求失败");
    for base in SOURCES {
        let url = format!("{base}/ggml-{name}.bin");
        let mut resp = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { last_err = format!("请求失败（HTTP {}）", r.status().as_u16()); continue; }
            Err(_) => { last_err = "网络异常，无法下载语音模型".into(); continue; }
        };
        let total = resp.content_length().unwrap_or(0);
        let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut received = 0u64;
        let mut last_emit = 0u64;
        let ok = loop {
            if app.state::<VoiceState>().cancel.load(Ordering::SeqCst) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err("已取消下载".into());
            }
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    file.write_all(&bytes).map_err(|e| e.to_string())?;
                    received += bytes.len() as u64;
                    if received - last_emit > 512 * 1024 || received == total {
                        last_emit = received;
                        let _ = app.emit("voice:progress", Progress { name: name.into(), received, total });
                    }
                }
                Ok(None) => break true,
                Err(_) => { last_err = "下载中断，请重试".into(); break false; }
            }
        };
        drop(file);
        if ok && (total == 0 || received == total) && received > 1_000_000 {
            std::fs::rename(&part, path).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let _ = std::fs::remove_file(&part);
    }
    Err(last_err)
}

#[tauri::command]
pub fn voice_cancel_download(app: AppHandle) {
    app.state::<VoiceState>().cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn voice_delete(app: AppHandle, name: String) -> Result<(), String> {
    let p = model_path(&app, &name)?;
    {
        let st = app.state::<VoiceState>();
        let mut c = st.ctx.lock().unwrap();
        if c.as_ref().map(|(n, _)| n == &name).unwrap_or(false) {
            *c = None;
        }
    }
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("voice:changed", ());
    Ok(())
}

/// Start recording from the default input device.
#[tauri::command]
pub fn voice_start(app: AppHandle) -> Result<(), String> {
    let model = selected_model(&app);
    if !model_path(&app, &model)?.exists() {
        return Err("请先在设置中下载语音模型".into());
    }
    let st = app.state::<VoiceState>();
    let mut rec = st.rec.lock().unwrap();
    if rec.is_some() {
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let (tx, rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let (stop2, samples2, app2) = (stop.clone(), samples.clone(), app.clone());
    // cpal streams are not Send, so the stream lives on its own thread.
    let thread = std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(dev) = host.default_input_device() else {
            let _ = tx.send(Err("找不到麦克风".into()));
            return;
        };
        let cfg = match dev.default_input_config() {
            Ok(c) => c,
            Err(e) => { let _ = tx.send(Err(format!("无法打开麦克风：{e}"))); return; }
        };
        let ch = cfg.channels() as usize;
        let rate = cfg.sample_rate().0;
        let err_fn = |e| eprintln!("audio error: {e}");
        let level_at = Arc::new(Mutex::new(std::time::Instant::now()));
        macro_rules! input {
            ($t:ty, $conv:expr) => {{
                let s = samples2.clone();
                let app3 = app2.clone();
                let level_at = level_at.clone();
                dev.build_input_stream(
                    &cfg.clone().into(),
                    move |data: &[$t], _| {
                        let mut buf = s.lock().unwrap();
                        let mut peak = 0f32;
                        for frame in data.chunks(ch) {
                            let v = frame.iter().map(|x| $conv(*x)).sum::<f32>() / ch as f32;
                            peak = peak.max(v.abs());
                            buf.push(v);
                        }
                        let mut t = level_at.lock().unwrap();
                        if t.elapsed() > Duration::from_millis(80) {
                            *t = std::time::Instant::now();
                            let _ = app3.emit("voice:level", peak.min(1.0));
                        }
                    },
                    err_fn,
                    None,
                )
            }};
        }
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => input!(f32, |x: f32| x),
            cpal::SampleFormat::I16 => input!(i16, |x: i16| x as f32 / 32768.0),
            cpal::SampleFormat::U16 => input!(u16, |x: u16| (x as f32 - 32768.0) / 32768.0),
            cpal::SampleFormat::I32 => input!(i32, |x: i32| x as f32 / 2147483648.0),
            f => { let _ = tx.send(Err(format!("不支持的音频格式：{f:?}"))); return; }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => { let _ = tx.send(Err(format!("无法打开麦克风：{e}"))); return; }
        };
        if let Err(e) = stream.play() {
            let _ = tx.send(Err(format!("无法打开麦克风：{e}")));
            return;
        }
        let _ = tx.send(Ok(rate));
        // Hard cap of 60 s per utterance.
        let started = std::time::Instant::now();
        while !stop2.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(30));
        }
        drop(stream);
    });
    let rate = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "无法打开麦克风".to_string())??;
    *rec = Some(Recording { stop, samples, rate, thread });
    Ok(())
}

fn resample_to_16k(input: &[f32], rate: u32) -> Vec<f32> {
    if rate == 16_000 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = rate as f64 / 16_000.0;
    let n = (input.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let i0 = pos.floor() as usize;
            let i1 = (i0 + 1).min(input.len() - 1);
            let f = (pos - i0 as f64) as f32;
            input[i0] * (1.0 - f) + input[i1] * f
        })
        .collect()
}

/// Map our language codes to whisper's.
fn whisper_lang(code: &str) -> Option<&'static str> {
    Some(match code {
        "zh-Hans" | "zh-Hant" | "zh" => "zh",
        "en" => "en", "ja" => "ja", "ko" => "ko", "fr" => "fr", "es" => "es", "ru" => "ru",
        "ar" => "ar", "de" => "de", "it" => "it", "pt" => "pt", "id" => "id", "ms" => "ms",
        "hi" => "hi", "sv" => "sv", "th" => "th", "vi" => "vi", "tr" => "tr", "nl" => "nl",
        _ => return None,
    })
}

/// Stop recording and transcribe. `source` is the panel's source language ("auto" detects).
#[tauri::command]
pub async fn voice_stop(app: AppHandle, source: Option<String>) -> Result<String, String> {
    let rec = app.state::<VoiceState>().rec.lock().unwrap().take();
    let Some(rec) = rec else { return Ok(String::new()) };
    rec.stop.store(true, Ordering::SeqCst);
    let _ = rec.thread.join();
    let audio = std::mem::take(&mut *rec.samples.lock().unwrap());
    let rate = rec.rate;
    if audio.len() < (rate as usize) / 4 {
        return Ok(String::new());
    }
    let model = selected_model(&app);
    let path = model_path(&app, &model)?;
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let ctx = {
            let st = app2.state::<VoiceState>();
            let mut c = st.ctx.lock().unwrap();
            match c.as_ref() {
                Some((n, ctx)) if *n == model => ctx.clone(),
                _ => {
                    let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                        .map_err(|e| format!("无法加载语音模型：{e}"))?;
                    let ctx = Arc::new(ctx);
                    *c = Some((model.clone(), ctx.clone()));
                    ctx
                }
            }
        };
        let pcm = resample_to_16k(&audio, rate);
        let mut state = ctx.create_state().map_err(|e| e.to_string())?;
        let lang = source.as_deref().and_then(whisper_lang);
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(lang.unwrap_or("auto")));
        params.set_n_threads(std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_suppress_blank(true);
        if source.as_deref() == Some("zh-Hans") || source.as_deref() == Some("auto") || source.is_none() {
            // Nudges Chinese output toward Simplified characters with punctuation.
            params.set_initial_prompt("以下是普通话的句子，使用简体中文。");
        }
        state.full(params, &pcm).map_err(|e| format!("语音识别失败：{e}"))?;
        let mut text = String::new();
        for seg in state.as_iter() {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(s.trim());
                text.push(' ');
            }
        }
        Ok(text.trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
