// NexusCode proxy 0.2.0 — OpenAI-compatible POST /v1/chat/completions -> Gemini.
// Key Gemini chi song trong env server, khong bao gio ve client.
// Phase 1: KHONG yeu cau Bearer (app gui khong co Authorization header);
// chong lam dung bang rate-limit theo IP (30 req/phut). Phase 2: xac thuc
// token "Dang nhap bang Zhee" (OAuth) roi moi goi Gemini.

use axum::{
    extract::{ConnectInfo, Json, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

const DEFAULT_MODEL: &str = "gemini-2.5-flash";
const RATE_LIMIT_PER_MIN: u32 = 30;
const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

#[derive(Clone)]
struct AppState {
    gemini_key: String,
    http: reqwest::Client,
    hits: Arc<Mutex<HashMap<std::net::IpAddr, (Instant, u32)>>>,
}

// ---------- OpenAI-compatible shapes ----------

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatChoice {
    index: usize,
    message: ChatMessage,
    finish_reason: String,
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    id: String,
    object: String,
    model: String,
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Serialize)]
struct ChatChunkChoice {
    index: usize,
    delta: ChatDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatDelta {
    role: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatChunk {
    id: String,
    object: String,
    model: String,
    choices: Vec<ChatChunkChoice>,
}

// ---------- Gemini shapes ----------

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContentOut>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiContentOut {
    parts: Option<Vec<GeminiPartOut>>,
}

#[derive(Deserialize)]
struct GeminiPartOut {
    text: Option<String>,
}

fn gemini_text(resp: &GeminiResponse) -> (String, Option<String>) {
    let mut text = String::new();
    let mut finish = None;
    if let Some(cands) = &resp.candidates {
        if let Some(c) = cands.first() {
            finish = c.finish_reason.clone();
            if let Some(content) = &c.content {
                if let Some(parts) = &content.parts {
                    for p in parts {
                        if let Some(t) = &p.text {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
    }
    (text, finish)
}

/// Gemini doi luan phien user/model xen ke, mo dau phai la user.
/// Gop cac message cung role lien nhau, bo message model o dau.
fn normalize_contents(messages: &[ChatMessage]) -> (Vec<GeminiContent>, Option<String>) {
    let mut system: Option<String> = None;
    let mut merged: Vec<(String, String)> = Vec::new();
    for m in messages {
        match m.role.as_str() {
            "system" => {
                system = Some(match system {
                    Some(prev) => format!("{prev}\n\n{}", m.content),
                    None => m.content.clone(),
                });
            }
            "assistant" => {
                if let Some(last) = merged.last_mut() {
                    if last.0 == "model" {
                        last.1.push_str("\n\n");
                        last.1.push_str(&m.content);
                        continue;
                    }
                }
                merged.push(("model".to_string(), m.content.clone()));
            }
            _ => {
                if let Some(last) = merged.last_mut() {
                    if last.0 == "user" {
                        last.1.push_str("\n\n");
                        last.1.push_str(&m.content);
                        continue;
                    }
                }
                merged.push(("user".to_string(), m.content.clone()));
            }
        }
    }
    while merged.first().map(|(r, _)| r.as_str()) == Some("model") {
        merged.remove(0);
    }
    let contents = merged
        .into_iter()
        .map(|(role, text)| GeminiContent {
            role: Some(role),
            parts: vec![GeminiPart { text }],
        })
        .collect();
    (contents, system)
}

async fn health() -> &'static str {
    "ok"
}

fn rate_limited(state: &AppState, ip: std::net::IpAddr) -> bool {
    let mut hits = state.hits.lock().unwrap();
    let now = Instant::now();
    let entry = hits.entry(ip).or_insert((now, 0));
    if now.duration_since(entry.0) > Duration::from_secs(60) {
        *entry = (now, 0);
    }
    entry.1 += 1;
    entry.1 > RATE_LIMIT_PER_MIN
}

async fn chat_completions(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    if state.gemini_key.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "server key not configured"})),
        )
            .into_response();
    }
    if rate_limited(&state, addr.ip()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "rate limited, try again in a minute"})),
        )
            .into_response();
    }

    let model = if req.model.trim().is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        req.model.clone()
    };
    let (contents, system) = normalize_contents(&req.messages);
    if contents.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no user message"})),
        )
            .into_response();
    }
    let body = GeminiRequest {
        contents,
        system_instruction: system.map(|t| GeminiContent {
            role: None,
            parts: vec![GeminiPart { text: t }],
        }),
    };

    if req.stream {
        return stream_reply(state, model, body).await.into_response();
    }

    let url = format!("{GEMINI_BASE}/models/{model}:generateContent");
    let resp = match state
        .http
        .post(&url)
        .header("x-goog-api-key", &state.gemini_key)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("upstream unreachable: {e}")})),
            )
                .into_response();
        }
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let short: String = text.chars().take(300).collect();
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("gemini {status}: {short}")})),
        )
            .into_response();
    }
    let parsed: GeminiResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("bad upstream json: {e}")})),
            )
                .into_response();
        }
    };
    let (text, finish) = gemini_text(&parsed);
    let out = ChatResponse {
        id: format!("chatcmpl-{}", uuid_simple()),
        object: "chat.completion".to_string(),
        model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: text,
            },
            finish_reason: match finish.as_deref() {
                Some("STOP") => "stop".to_string(),
                Some(other) => other.to_lowercase(),
                None => "stop".to_string(),
            },
        }],
    };
    (StatusCode::OK, Json(out)).into_response()
}

async fn stream_reply(
    state: AppState,
    model: String,
    body: GeminiRequest,
) -> impl IntoResponse {
    let url = format!("{GEMINI_BASE}/models/{model}:streamGenerateContent?alt=sse");
    let resp = match state
        .http
        .post(&url)
        .header("x-goog-api-key", &state.gemini_key)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("upstream unreachable: {e}")})),
            )
                .into_response();
        }
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let short: String = text.chars().take(300).collect();
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("gemini {status}: {short}")})),
        )
            .into_response();
    }

    let id = format!("chatcmpl-{}", uuid_simple());
    let model_clone = model.clone();
    let reader = StreamReader::new(
        resp.bytes_stream()
            .map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))),
    );
    let mut lines = BufReader::new(reader).lines();
    let stream = async_stream::stream! {
        let mut first = true;
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        break;
                    }
                    let parsed: Result<GeminiResponse, _> = serde_json::from_str(data);
                    let Ok(parsed) = parsed else { continue };
                    let (text, finish) = gemini_text(&parsed);
                    if text.is_empty() && finish.is_none() {
                        continue;
                    }
                    let chunk = ChatChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk".to_string(),
                        model: model_clone.clone(),
                        choices: vec![ChatChunkChoice {
                            index: 0,
                            delta: ChatDelta {
                                role: if first { Some("assistant".to_string()) } else { None },
                                content: if text.is_empty() { None } else { Some(text) },
                            },
                            finish_reason: match finish.as_deref() {
                                Some("STOP") => Some("stop".to_string()),
                                Some(other) => Some(other.to_lowercase()),
                                None => None,
                            },
                        }],
                    };
                    first = false;
                    if let Ok(json) = serde_json::to_string(&chunk) {
                        yield Ok::<Event, axum::Error>(Event::default().data(json));
                    }
                    if finish.is_some() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        yield Ok::<Event, axum::Error>(Event::default().data("[DONE]"));
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn uuid_simple() -> String {
    // id duy nhat kieu hex, khong can them crate uuid
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    format!("{:x}{:x}", nanos, pid)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("nexuscode_proxy=debug,tower_http=debug")
        .init();

    let state = AppState {
        gemini_key: std::env::var("GEMINI_API_KEY").unwrap_or_default(),
        http: reqwest::Client::new(),
        hits: Arc::new(Mutex::new(HashMap::new())),
    };
    if state.gemini_key.is_empty() {
        tracing::warn!("GEMINI_API_KEY chua dat — /v1/chat/completions se tra 503");
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("nexuscode-proxy listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
