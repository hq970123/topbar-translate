//! Translation adapters. Each provider kind gets its own request/response shape
//! and auth scheme; errors are normalised into user-facing categories.

use crate::config::Provider;
use crate::lang;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Serialize, Debug, Clone)]
pub struct ApiError {
    /// network | timeout | rate_limit | auth | quota | model | server | unsupported | empty | config | bad_response
    pub code: String,
    pub message: String,
    pub detail: String,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), detail: String::new() }
    }
    fn with_detail(mut self, d: impl Into<String>) -> Self {
        let mut d: String = d.into();
        if d.len() > 400 {
            let mut cut = 400;
            while !d.is_char_boundary(cut) {
                cut -= 1;
            }
            d.truncate(cut);
        }
        self.detail = d;
        self
    }
}

pub struct Job<'a> {
    pub text: &'a str,
    /// data:image/...;base64,... (pasted image) or None
    pub image: Option<&'a str>,
    pub source: &'a str,
    pub target: &'a str,
}

pub fn supports_image(kind: &str) -> bool {
    matches!(kind, "index" | "openai" | "gemini" | "claude" | "custom")
}

fn text_prompt(job: &Job) -> String {
    format!(
        "请将以下文本翻译为{}，直接输出翻译结果，不要进行任何解释。\n\n{}",
        lang::prompt_name(job.target),
        job.text
    )
}

fn image_prompt(job: &Job) -> String {
    format!(
        "请识别图片中的文字并翻译为{}，直接输出翻译结果，不要进行任何解释。",
        lang::prompt_name(job.target)
    )
}

fn split_data_url(d: &str) -> Option<(String, String)> {
    let rest = d.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    Some((mime.to_string(), b64.to_string()))
}

fn clean_output(s: &str) -> String {
    let mut t = s.to_string();
    if let Some(idx) = t.find("</think>") {
        t = t[idx + "</think>".len()..].to_string();
    }
    t.trim().trim_start_matches("<think>").trim().to_string()
}

fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn http_error(status: u16, body: &str, kind: &str) -> ApiError {
    let lower = body.to_lowercase();
    let e = if status == 429 && (lower.contains("quota") || lower.contains("insufficient")) {
        ApiError::new("quota", "API 额度不足，请充值或更换翻译服务")
    } else if status == 429 || status == 412 {
        if kind == "index" {
            ApiError::new("rate_limit", "免费接口请求过于频繁，请稍后再试或切换翻译源")
        } else {
            ApiError::new("rate_limit", "请求过于频繁，请稍后再试")
        }
    } else if status == 401 || status == 403 {
        ApiError::new("auth", "API Key 无效或无权限，请在设置中重新配置")
    } else if status == 402 || lower.contains("insufficient_quota") || lower.contains("quota exceeded") || lower.contains("balance") {
        ApiError::new("quota", "API 额度不足，请充值或更换翻译服务")
    } else if status == 404 || (lower.contains("model") && (lower.contains("not found") || lower.contains("does not exist") || lower.contains("not exist"))) {
        ApiError::new("model", "模型不可用，请检查模型名称或 Base URL")
    } else if status >= 500 {
        ApiError::new("server", format!("翻译服务暂时不可用（HTTP {status}），可重试或切换翻译源"))
    } else {
        ApiError::new("server", format!("请求失败（HTTP {status}）"))
    };
    e.with_detail(body.to_string())
}

fn send_error(e: reqwest::Error) -> ApiError {
    if e.is_timeout() {
        ApiError::new("timeout", "请求超时，请重试")
    } else if e.is_connect() {
        ApiError::new("network", "网络异常，无法连接翻译服务")
    } else {
        ApiError::new("network", "网络请求失败").with_detail(e.to_string())
    }
}

async fn post(
    client: &reqwest::Client,
    kind: &str,
    url: &str,
    headers: Vec<(&str, String)>,
    body: Value,
) -> Result<Value, ApiError> {
    let mut req = client.post(url).json(&body);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(send_error)?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(send_error)?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &text, kind));
    }
    serde_json::from_str(&text)
        .map_err(|_| ApiError::new("bad_response", "翻译服务返回了无法解析的内容").with_detail(text))
}

fn pick(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = match p.parse::<usize>() {
            Ok(i) => cur.get(i)?,
            Err(_) => cur.get(*p)?,
        };
    }
    cur.as_str().map(|s| s.to_string())
}

pub async fn run(p: &Provider, key: Option<String>, job: &Job<'_>) -> Result<String, ApiError> {
    if job.text.trim().is_empty() && job.image.is_none() {
        return Err(ApiError::new("empty", "请输入需要翻译的内容"));
    }
    if job.image.is_some() && !supports_image(&p.kind) {
        return Err(ApiError::new(
            "unsupported",
            format!("「{}」不支持图片翻译，请切换到 Index-Translate", p.name),
        ));
    }
    if crate::config::key_required(&p.kind) && key.is_none() {
        return Err(ApiError::new("auth", format!("「{}」尚未配置 API Key", p.name)));
    }
    crate::config::validate_base_url(&p.base_url).map_err(|m| ApiError::new("config", m))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(p.timeout_secs.clamp(5, 300)))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(if p.kind == "index" { "Index-Translate-Client/1.0" } else { "TopBarTranslate/1.0" })
        .build()
        .map_err(|e| ApiError::new("network", e.to_string()))?;

    let out = match p.kind.as_str() {
        "gemini" => gemini(&client, p, key.unwrap_or_default(), job).await?,
        "claude" => claude(&client, p, key.unwrap_or_default(), job).await?,
        "deepl" => deepl(&client, p, key.unwrap_or_default(), job).await?,
        "microsoft" => microsoft(&client, p, key.unwrap_or_default(), job).await?,
        _ => openai_compatible(&client, p, key, job).await?,
    };
    let out = clean_output(&out);
    if out.is_empty() {
        return Err(ApiError::new("bad_response", "翻译服务返回了空结果，请重试"));
    }
    Ok(out)
}

async fn openai_compatible(
    client: &reqwest::Client,
    p: &Provider,
    key: Option<String>,
    job: &Job<'_>,
) -> Result<String, ApiError> {
    let content = match job.image {
        Some(img) => json!([
            {"type": "image_url", "image_url": {"url": img}},
            {"type": "text", "text": image_prompt(job)}
        ]),
        None => json!(text_prompt(job)),
    };
    let mut body = json!({
        "model": p.model,
        "messages": [{"role": "user", "content": content}],
        "temperature": 0,
        "stream": false,
    });
    if p.kind == "index" {
        body["max_tokens"] = json!(1024);
        body["chat_template_kwargs"] = json!({"enable_thinking": false});
    } else {
        body["max_tokens"] = json!(4096);
    }
    let mut headers = vec![];
    if let Some(k) = key {
        headers.push(("Authorization", format!("Bearer {k}")));
    }
    let v = post(client, &p.kind, &join(&p.base_url, "chat/completions"), headers, body).await?;
    pick(&v, &["choices", "0", "message", "content"])
        .ok_or_else(|| ApiError::new("bad_response", "翻译服务返回格式异常").with_detail(v.to_string()))
}

async fn gemini(client: &reqwest::Client, p: &Provider, key: String, job: &Job<'_>) -> Result<String, ApiError> {
    let parts = match job.image.and_then(split_data_url) {
        Some((mime, data)) => json!([
            {"inline_data": {"mime_type": mime, "data": data}},
            {"text": image_prompt(job)}
        ]),
        None => json!([{"text": text_prompt(job)}]),
    };
    let body = json!({
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": {"temperature": 0}
    });
    let url = join(&p.base_url, &format!("models/{}:generateContent", p.model));
    let v = post(client, "gemini", &url, vec![("x-goog-api-key", key)], body).await?;
    let parts = v
        .pointer("/candidates/0/content/parts")
        .and_then(|x| x.as_array())
        .ok_or_else(|| ApiError::new("bad_response", "Gemini 返回格式异常").with_detail(v.to_string()))?;
    Ok(parts
        .iter()
        .filter_map(|x| x.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join(""))
}

async fn claude(client: &reqwest::Client, p: &Provider, key: String, job: &Job<'_>) -> Result<String, ApiError> {
    let content = match job.image.and_then(split_data_url) {
        Some((mime, data)) => json!([
            {"type": "image", "source": {"type": "base64", "media_type": mime, "data": data}},
            {"type": "text", "text": image_prompt(job)}
        ]),
        None => json!(text_prompt(job)),
    };
    let body = json!({
        "model": p.model,
        "max_tokens": 4096,
        "temperature": 0,
        "messages": [{"role": "user", "content": content}]
    });
    let headers = vec![("x-api-key", key), ("anthropic-version", "2023-06-01".to_string())];
    let v = post(client, "claude", &join(&p.base_url, "messages"), headers, body).await?;
    let blocks = v
        .get("content")
        .and_then(|x| x.as_array())
        .ok_or_else(|| ApiError::new("bad_response", "Claude 返回格式异常").with_detail(v.to_string()))?;
    Ok(blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join(""))
}

fn deepl_code(code: &str, target: bool) -> String {
    match code {
        "zh-Hans" => if target { "ZH-HANS".into() } else { "ZH".into() },
        "zh-Hant" => if target { "ZH-HANT".into() } else { "ZH".into() },
        "en" => if target { "EN-US".into() } else { "EN".into() },
        "pt" => if target { "PT-BR".into() } else { "PT".into() },
        c => c.to_uppercase(),
    }
}

async fn deepl(client: &reqwest::Client, p: &Provider, key: String, job: &Job<'_>) -> Result<String, ApiError> {
    let mut body = json!({"text": [job.text], "target_lang": deepl_code(job.target, true)});
    if job.source != "auto" && !job.source.is_empty() {
        body["source_lang"] = json!(deepl_code(job.source, false));
    }
    let headers = vec![("Authorization", format!("DeepL-Auth-Key {key}"))];
    let v = post(client, "deepl", &join(&p.base_url, "translate"), headers, body).await?;
    pick(&v, &["translations", "0", "text"])
        .ok_or_else(|| ApiError::new("bad_response", "DeepL 返回格式异常").with_detail(v.to_string()))
}

async fn microsoft(client: &reqwest::Client, p: &Provider, key: String, job: &Job<'_>) -> Result<String, ApiError> {
    let mut url = url::Url::parse(&join(&p.base_url, "translate"))
        .map_err(|_| ApiError::new("config", "Base URL 格式不正确"))?;
    url.query_pairs_mut().append_pair("api-version", "3.0").append_pair("to", job.target);
    if job.source != "auto" && !job.source.is_empty() {
        url.query_pairs_mut().append_pair("from", job.source);
    }
    let mut headers = vec![("Ocp-Apim-Subscription-Key", key)];
    if !p.region.trim().is_empty() {
        headers.push(("Ocp-Apim-Subscription-Region", p.region.trim().to_string()));
    }
    let v = post(client, "microsoft", url.as_str(), headers, json!([{"Text": job.text}])).await?;
    pick(&v, &["0", "translations", "0", "text"])
        .ok_or_else(|| ApiError::new("bad_response", "Microsoft Translator 返回格式异常").with_detail(v.to_string()))
}
