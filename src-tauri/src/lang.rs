//! Language list and automatic language detection.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Language {
    pub code: &'static str,
    /// Name shown in the UI.
    pub label: &'static str,
    /// Name used inside translation prompts.
    pub prompt: &'static str,
    /// BCP-47 tag for speech synthesis.
    pub speech: &'static str,
}

macro_rules! l {
    ($c:expr, $l:expr, $p:expr, $s:expr) => {
        Language { code: $c, label: $l, prompt: $p, speech: $s }
    };
}

pub const LANGUAGES: &[Language] = &[
    l!("zh-Hans", "中文（简体）", "简体中文", "zh-CN"),
    l!("zh-Hant", "中文（繁体）", "繁体中文", "zh-TW"),
    l!("en", "English", "英语", "en-US"),
    l!("ja", "日本語", "日语", "ja-JP"),
    l!("ko", "한국어", "韩语", "ko-KR"),
    l!("fr", "Français", "法语", "fr-FR"),
    l!("es", "Español", "西班牙语", "es-ES"),
    l!("de", "Deutsch", "德语", "de-DE"),
    l!("ru", "Русский", "俄语", "ru-RU"),
    l!("pt", "Português", "葡萄牙语", "pt-BR"),
    l!("it", "Italiano", "意大利语", "it-IT"),
    l!("ar", "العربية", "阿拉伯语", "ar-SA"),
    l!("th", "ไทย", "泰语", "th-TH"),
    l!("vi", "Tiếng Việt", "越南语", "vi-VN"),
    l!("id", "Bahasa Indonesia", "印尼语", "id-ID"),
    l!("ms", "Bahasa Melayu", "马来语", "ms-MY"),
    l!("tr", "Türkçe", "土耳其语", "tr-TR"),
    l!("hi", "हिन्दी", "印地语", "hi-IN"),
    l!("nl", "Nederlands", "荷兰语", "nl-NL"),
    l!("pl", "Polski", "波兰语", "pl-PL"),
    l!("uk", "Українська", "乌克兰语", "uk-UA"),
    l!("sv", "Svenska", "瑞典语", "sv-SE"),
];

pub fn find(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code))
}

pub fn prompt_name(code: &str) -> String {
    find(code).map(|l| l.prompt.to_string()).unwrap_or_else(|| code.to_string())
}

pub fn is_chinese(code: &str) -> bool {
    code.starts_with("zh")
}

// Characters that only appear in Traditional Chinese (a small, common subset).
const TRAD_ONLY: &str = "們這個來時說對國學會麼發電體點經過還開關問題樣讓從後實現當還應該與無門見長東車書買賣聽寫認識頭裡錢鐘氣愛覺為產幾區親間處邊動將機場網頁";
const SIMP_ONLY: &str = "们这个来时说对国学会么发电体点经过还开关问题样让从后实现当还应该与无门见长东车书买卖听写认识头里钱钟气爱觉为产几区亲间处边动将机场网页";

/// Detect the language of `text`, returning one of the codes in [`LANGUAGES`].
pub fn detect(text: &str) -> String {
    let mut han = 0usize;
    let mut kana = 0usize;
    let mut hangul = 0usize;
    let mut letters = 0usize;
    let mut trad = 0usize;
    let mut simp = 0usize;
    for c in text.chars() {
        let u = c as u32;
        if c.is_alphabetic() {
            letters += 1;
        }
        match u {
            0x3040..=0x30FF | 0x31F0..=0x31FF => kana += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => hangul += 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => {
                han += 1;
                if TRAD_ONLY.contains(c) {
                    trad += 1;
                } else if SIMP_ONLY.contains(c) {
                    simp += 1;
                }
            }
            _ => {}
        }
    }
    if letters == 0 {
        return "en".into();
    }
    if kana > 0 && kana * 10 >= (han + kana) {
        return "ja".into();
    }
    if hangul > 0 && hangul * 2 >= letters / 2 {
        return "ko".into();
    }
    if han * 10 >= letters * 3 {
        return if trad > simp { "zh-Hant".into() } else { "zh-Hans".into() };
    }
    let ascii_only = text.chars().all(|c| c.is_ascii());
    if ascii_only && english_score(text) >= 0.25 {
        return "en".into();
    }
    use whatlang::Lang::*;
    let allow = vec![Eng, Fra, Spa, Deu, Rus, Por, Ita, Ara, Tha, Vie, Ind, Tur, Hin, Nld, Pol, Ukr, Swe];
    let detector = whatlang::Detector::with_allowlist(allow);
    if let Some(info) = detector.detect(text) {
        let code = match info.lang() {
            Eng => "en",
            Fra => "fr",
            Spa => "es",
            Deu => "de",
            Rus => "ru",
            Por => "pt",
            Ita => "it",
            Ara => "ar",
            Tha => "th",
            Vie => "vi",
            Ind => "id",
            Tur => "tr",
            Hin => "hi",
            Nld => "nl",
            Pol => "pl",
            Ukr => "uk",
            Swe => "sv",
            _ => "",
        };
        // Very short ASCII snippets are unreliable; English is by far the most likely.
        if ascii_only && (code.is_empty() || info.confidence() < 0.05) {
            return "en".into();
        }
        if !code.is_empty() {
            return code.into();
        }
    }
    if ascii_only {
        "en".into()
    } else {
        "auto".into()
    }
}

const EN_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "at", "for", "with", "is", "are", "was", "were", "be", "been",
    "i", "you", "he", "she", "it", "we", "they", "me", "my", "your", "our", "their", "this", "that", "these", "those",
    "what", "how", "why", "when", "where", "who", "which", "do", "does", "did", "have", "has", "had", "not", "no", "yes",
    "can", "could", "will", "would", "should", "please", "thank", "thanks", "hello", "hi", "hey", "good", "morning",
    "night", "today", "very", "much", "love", "like", "just", "about", "from", "there", "here", "so", "if", "but", "all",
    "get", "got", "make", "know", "think", "want", "need", "time", "day", "new", "app", "work", "use", "sorry", "ok", "okay",
];

fn english_score(text: &str) -> f64 {
    let words: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphabetic() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    if words.is_empty() {
        return 0.0;
    }
    let hits = words.iter().filter(|w| EN_WORDS.contains(&w.as_str())).count();
    hits as f64 / words.len() as f64
}

/// Resolve the target language following the PRD rule:
/// Chinese → second target (English by default), everything else → primary target (Chinese).
pub fn resolve_target(source: &str, requested: &str, primary: &str, secondary: &str) -> String {
    if requested != "auto" && !requested.is_empty() {
        if requested == source {
            // Same language both sides: flip to the other default.
            return if requested == primary { secondary.into() } else { primary.into() };
        }
        return requested.into();
    }
    if is_chinese(source) && is_chinese(primary) {
        secondary.into()
    } else if source == primary {
        secondary.into()
    } else {
        primary.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_common() {
        assert_eq!(detect("你好，今天过得怎么样？"), "zh-Hans");
        assert_eq!(detect("你們這個問題怎麼樣"), "zh-Hant");
        assert_eq!(detect("Hello, how are you today?"), "en");
        assert_eq!(detect("Hello"), "en");
        assert_eq!(detect("good morning"), "en");
        assert_eq!(detect("Hola, ¿cómo estás?"), "es");
        assert_eq!(detect("Ciao, come stai oggi?"), "it");
        assert_eq!(detect("The quick brown fox jumps over the lazy dog"), "en");
        assert_eq!(detect("Wie geht es dir heute?"), "de");
        assert_eq!(detect("こんにちは、元気ですか"), "ja");
        assert_eq!(detect("안녕하세요 반갑습니다"), "ko");
        assert_eq!(detect("Bonjour, comment allez-vous aujourd'hui ?"), "fr");
        assert_eq!(resolve_target("zh-Hans", "auto", "zh-Hans", "en"), "en");
        assert_eq!(resolve_target("en", "auto", "zh-Hans", "en"), "zh-Hans");
        assert_eq!(resolve_target("ja", "auto", "zh-Hans", "en"), "zh-Hans");
    }
}
