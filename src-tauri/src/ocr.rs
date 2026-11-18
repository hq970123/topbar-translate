//! On-device text recognition for images, using the OCR built into the OS:
//! Windows.Media.Ocr on Windows 10/11 and the Vision framework on macOS.
//! Nothing leaves the machine; only the recognized text is translated.
use base64::Engine;

/// Longest side we hand to the OS engine (Windows.Media.Ocr refuses > 2600 px).
const MAX_SIDE: u32 = 2600;

/// Recognize the text in a `data:image/...;base64,...` URL.
pub fn recognize_data_url(url: &str) -> Result<String, String> {
    let b64 = url.split_once(",").map(|x| x.1).ok_or("图片格式无法识别")?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| "图片格式无法识别".to_string())?;
    let png = normalize(&raw).unwrap_or(raw);
    let text = platform::recognize(&png)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("图片中没有识别到文字".into());
    }
    Ok(text)
}

/// Decode and re-encode as PNG, shrinking anything larger than the engine accepts.
fn normalize(raw: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(raw).ok()?;
    let img = if img.width().max(img.height()) > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

/// CJK scripts are written without spaces between words.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF |
        0xAC00..=0xD7AF | 0x3000..=0x303F | 0xFF00..=0xFFEF)
}

/// Join recognized words, leaving out the space between two CJK words.
#[allow(dead_code)]
pub(crate) fn join_words<I: IntoIterator<Item = String>>(words: I) -> String {
    let mut s = String::new();
    for w in words {
        let w = w.trim();
        if w.is_empty() {
            continue;
        }
        let gap = match (s.chars().last(), w.chars().next()) {
            (Some(a), Some(b)) => !(is_cjk(a) && is_cjk(b)),
            _ => false,
        };
        if gap {
            s.push(' ');
        }
        s.push_str(w);
    }
    s
}

#[cfg(windows)]
mod platform {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    pub fn recognize(png: &[u8]) -> Result<String, String> {
        run(png).map_err(|e| format!("系统 OCR 识别失败：{}", e.message()))?
    }

    fn run(png: &[u8]) -> windows::core::Result<Result<String, String>> {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(png)?;
        writer.StoreAsync()?.get()?;
        writer.FlushAsync()?.get()?;
        writer.DetachStream()?;
        stream.Seek(0)?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
        let bitmap = decoder.GetSoftwareBitmapAsync()?.get()?;
        let engine = match OcrEngine::TryCreateFromUserProfileLanguages() {
            Ok(e) => e,
            Err(_) => {
                return Ok(Err(
                    "系统没有可用的 OCR 语言包：请在 Windows 设置 › 时间和语言 › 语言 中为语言安装「光学字符识别」".into(),
                ))
            }
        };
        let result = engine.RecognizeAsync(&bitmap)?.get()?;
        let mut lines = Vec::new();
        for line in result.Lines()? {
            let words: Vec<String> = line.Words()?.into_iter().filter_map(|w| w.Text().ok()).map(|h| h.to_string()).collect();
            lines.push(super::join_words(words));
        }
        Ok(Ok(lines.join("\n")))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
    use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};

    pub fn recognize(png: &[u8]) -> Result<String, String> {
        // Newer macOS versions know more languages; fall back to the set every version has.
        let sets: [&[&str]; 2] = [&["zh-Hans", "zh-Hant", "en-US", "ja-JP", "ko-KR"], &["zh-Hans", "en-US"]];
        let mut last = String::new();
        for langs in sets {
            match run(png, langs) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(format!("系统 OCR 识别失败：{last}"))
    }

    fn run(png: &[u8], langs: &[&str]) -> Result<String, String> {
        objc2::rc::autoreleasepool(|_| {
            let data = NSData::with_bytes(png);
            let opts = NSDictionary::new();
            let handler = VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &opts);
            let req = VNRecognizeTextRequest::new();
            req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            req.setUsesLanguageCorrection(true);
            let names: Vec<Retained<NSString>> = langs.iter().map(|l| NSString::from_str(l)).collect();
            req.setRecognitionLanguages(&NSArray::from_retained_slice(&names));
            let as_req: Retained<VNRequest> = Retained::into_super(Retained::into_super(req.clone()));
            handler
                .performRequests_error(&NSArray::from_retained_slice(&[as_req]))
                .map_err(|e| e.localizedDescription().to_string())?;
            let mut lines = Vec::new();
            if let Some(obs) = req.results() {
                for o in obs.iter() {
                    if let Some(c) = o.topCandidates(1).firstObject() {
                        lines.push(c.string().to_string());
                    }
                }
            }
            Ok(lines.join("\n"))
        })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn recognize(_png: &[u8]) -> Result<String, String> {
        Err("当前系统不支持本地 OCR".into())
    }
}

#[cfg(test)]
mod tests {
    use super::join_words;
    #[test]
    fn joins_cjk_without_spaces() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(join_words(v(&["你好", "世界"])), "你好世界");
        assert_eq!(join_words(v(&["Hello", "world"])), "Hello world");
        assert_eq!(join_words(v(&["使用", "Rust", "开发"])), "使用 Rust 开发");
    }
}
