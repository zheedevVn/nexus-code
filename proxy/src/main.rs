// NexusCode proxy 0.3.0 — OpenAI-compatible POST /v1/chat/completions -> Gemini.
// LOGIN-ONLY: khong con API key. App dang nhap Google, gui
// `Authorization: Bearer <access_token>`; proxy chuyen tiep token do + header
// `x-goog-user-project` sang Gemini. Quota tinh theo tai khoan Google cua
// tung nguoi dung (free tier), khong phai theo key server.
// Chong lam dung bang rate-limit theo IP (30 req/phut).

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
    /// So project Google Cloud (public, khong phai secret). Gui kem moi
    /// request qua header `x-goog-user-project`; uu tien hon header cung
    /// ten do client gui len.
    project_number: String,
    /// Cho phep tro sang Gemini gia lap khi test (env GEMINI_BASE).
    gemini_base: String,
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
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    // LOGIN-ONLY: can Bearer token tu client; khong co -> 401.
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    let Some(token) = bearer else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "login required"})),
        )
            .into_response();
    };
    // x-goog-user-project: env server uu tien, neu khong co thi lay header
    // do client gui (app lay tu vsix contribution).
    let user_project = if !state.project_number.is_empty() {
        Some(state.project_number.clone())
    } else {
        headers
            .get("x-goog-user-project")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
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
        return stream_reply(state, model, body, token, user_project)
            .await
            .into_response();
    }

    let url = format!("{}/models/{model}:generateContent", state.gemini_base);
    let mut rb = state
        .http
        .post(&url)
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(project) = &user_project {
        rb = rb.header("x-goog-user-project", project.clone());
    }
    let resp = match rb.json(&body).send().await {
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
    token: String,
    user_project: Option<String>,
) -> impl IntoResponse {
    let url = format!(
        "{}/models/{model}:streamGenerateContent?alt=sse",
        state.gemini_base
    );
    let mut rb = state
        .http
        .post(&url)
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(project) = &user_project {
        rb = rb.header("x-goog-user-project", project.clone());
    }
    let resp = match rb.json(&body).send().await {
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
        project_number: std::env::var("GCP_PROJECT_NUMBER").unwrap_or_default(),
        gemini_base: std::env::var("GEMINI_BASE").unwrap_or_else(|_| GEMINI_BASE.to_string()),
        http: reqwest::Client::new(),
        hits: Arc::new(Mutex::new(HashMap::new())),
    };
    if state.project_number.is_empty() {
        tracing::warn!("GCP_PROJECT_NUMBER chua dat — request se thieu x-goog-user-project (app phai gui header nay)");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn normalize_merges_same_role_and_drops_leading_model() {
        let (contents, system) = normalize_contents(&[
            msg("system", "You are helpful"),
            msg("assistant", "leading model to drop"),
            msg("user", "hi"),
            msg("user", "again"),
            msg("assistant", "reply"),
        ]);
        assert_eq!(system.as_deref(), Some("You are helpful"));
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert_eq!(contents[0].parts[0].text, "hi\n\nagain");
        assert_eq!(contents[1].role.as_deref(), Some("model"));
        assert_eq!(contents[1].parts[0].text, "reply");
    }

    #[test]
    fn normalize_empty_messages() {
        let (contents, system) = normalize_contents(&[]);
        assert!(contents.is_empty());
        assert!(system.is_none());
    }
}
