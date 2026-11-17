//! Single-word dictionary lookup (English) via the free dictionaryapi.dev service.
//! Only called for one English word when "word lookup" is on; failures are silent.

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Serialize, Default)]
pub struct Meaning {
    pub pos: String,
    pub defs: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct Entry {
    pub word: String,
    pub phonetic: String,
    pub audio: String,
    pub meanings: Vec<Meaning>,
}

fn is_word(w: &str) -> bool {
    let n = w.chars().count();
    (1..=40).contains(&n)
        && w.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && w.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'')
}

pub async fn lookup(word: &str) -> Option<Entry> {
    let word = word.trim().to_lowercase();
    if !is_word(&word) {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .user_agent("TopBarTranslate/1.0")
        .build()
        .ok()?;
    let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{word}");
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    parse(&v)
}

fn parse(v: &Value) -> Option<Entry> {
    let list = v.as_array()?;
    let first = list.first()?;
    let mut e = Entry { word: first["word"].as_str().unwrap_or_default().to_string(), ..Default::default() };
    for item in list {
        if e.phonetic.is_empty() {
            if let Some(p) = item["phonetic"].as_str() {
                e.phonetic = p.to_string();
            }
        }
        for ph in item["phonetics"].as_array().into_iter().flatten() {
            if e.phonetic.is_empty() {
                if let Some(t) = ph["text"].as_str().filter(|t| !t.is_empty()) {
                    e.phonetic = t.to_string();
                }
            }
            if let Some(a) = ph["audio"].as_str().filter(|a| a.starts_with("https://")) {
                // Prefer US pronunciation when several are given.
                if e.audio.is_empty() || a.contains("-us.") {
                    e.audio = a.to_string();
                }
            }
        }
        for m in item["meanings"].as_array().into_iter().flatten() {
            if e.meanings.len() >= 4 {
                break;
            }
            let pos = m["partOfSpeech"].as_str().unwrap_or_default().to_string();
            let defs: Vec<String> = m["definitions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d["definition"].as_str().map(str::to_string))
                .take(2)
                .collect();
            if defs.is_empty() || e.meanings.iter().any(|x| x.pos == pos) {
                continue;
            }
            e.meanings.push(Meaning { pos, defs });
        }
    }
    if e.word.is_empty() || (e.meanings.is_empty() && e.phonetic.is_empty()) {
        return None;
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn words() {
        assert!(is_word("hello"));
        assert!(is_word("well-being"));
        assert!(!is_word("hello world"));
        assert!(!is_word("你好"));
        assert!(!is_word("../x"));
    }
    #[test]
    fn parses() {
        let v: Value = serde_json::from_str(r#"[{"word":"hello","phonetics":[{"text":"/həˈləʊ/","audio":"https://api.dictionaryapi.dev/media/pronunciations/en/hello-uk.mp3"},{"audio":"https://x/hello-us.mp3"}],"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"A greeting."}]},{"partOfSpeech":"verb","definitions":[{"definition":"To greet."}]}]}]"#).unwrap();
        let e = parse(&v).unwrap();
        assert_eq!(e.phonetic, "/həˈləʊ/");
        assert!(e.audio.ends_with("hello-us.mp3"));
        assert_eq!(e.meanings.len(), 2);
    }
}
