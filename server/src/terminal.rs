//! GET /ws/term —— 对应 webui 的 guest 串口终端。
//!
//! 三件事：独占订阅（第二连接 409）、帧协议 v1（Binary=数据 / Text=控制）、
//! 背压演示（暂停时停止消费，让通道积压溢出，生产者只丢不堵）。

use std::time::Duration;

use axum::{
    body::Bytes,
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        FromRequestParts, Query, Request, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    state::{AppState, Evidence, HelloSubscription},
    TOKEN,
};

/// 有数据流量时重置计时，所以空闲才 ping。
const PING_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// 浏览器 ws 发不了自定义 header，所以 token 走查询参数（§4.3）。
///
/// axum 0.8 的 `Option<WebSocketUpgrade>` 不可用（要求 OptionalFromRequestParts），
/// 所以这里拿原始 Request 自己判断是否升级请求——这也正好让探测请求
/// （不带 Upgrade 头的普通 GET）与真升级走同一个 handler。
pub async fn ws_term(
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    req: Request,
) -> Response {
    if q.token.as_deref() != Some(TOKEN) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // 独占检查在 upgrade 之前：已被占用就直接 409（不变量 4）
    if state.hello.is_busy().await {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "terminal busy" })),
        )
            .into_response();
    }

    let (mut parts, _body) = req.into_parts();

    // 没有 Upgrade 头的普通 GET 是前端的探测请求——浏览器 WebSocket 拿不到握手
    // 状态码，只能先用 fetch 问一次「订阅位空着吗」。走到这里说明是空的。
    if !is_websocket_upgrade(&parts.headers) {
        return upgrade_required();
    }

    let evidence = state.evidence.clone();
    let Some(sub) = state.hello.subscribe().await else {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "terminal busy" })),
        )
            .into_response();
    };

    match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws.on_upgrade(move |socket| session(socket, sub, evidence)),
        Err(_) => upgrade_required(),
    }
}

fn upgrade_required() -> Response {
    (
        StatusCode::UPGRADE_REQUIRED,
        Json(json!({ "error": "upgrade required" })),
    )
        .into_response()
}

fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    let conn_upgrades = headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.to_ascii_lowercase()
                .split(',')
                .any(|p| p.trim().eq_ignore_ascii_case("upgrade"))
        });
    let upgrade_websocket = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    conn_upgrades && upgrade_websocket
}

async fn session(mut socket: WebSocket, mut sub: HelloSubscription, evidence: Evidence) {
    // 不变量 9：连接建立后先发 hello 控制帧
    if send_control(&mut socket, &json!({ "type": "hello", "proto": 1 }))
        .await
        .is_err()
    {
        return;
    }

    let mut paused = false;
    let mut reported: u64 = 0;
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();

    loop {
        tokio::select! {
            // 暂停时故意不 recv：让通道积压、溢出丢弃（背压演示的核心）
            line = sub.rx.recv(), if !paused => match line {
                Some(line) => {
                    evidence.console("OUT", &line);
                    if socket.send(Message::Binary(Bytes::from(line))).await.is_err() {
                        break;
                    }
                    ping.reset();
                }
                None => break,
            },

            msg = socket.recv() => match msg {
                Some(Ok(Message::Binary(data))) => {
                    // 回显：真实系统里由 guest 终端驱动做，传输层不回显；
                    // demo 由 server 代演，好让面板行为完整（§7）
                    let text = String::from_utf8_lossy(&data).to_string();
                    evidence.console("IN", &format!("{text:?}"));
                    if socket.send(Message::Binary(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => match client_control(&text) {
                    Some(ClientControl::Pause) => {
                        evidence.console("CTL", "pause");
                        paused = true;
                    }
                    Some(ClientControl::Resume) => {
                        evidence.console("CTL", "resume");
                        paused = false;
                        // 恢复后先发 dropped 帧，再恢复数据流
                        let now = sub.dropped.load(std::sync::atomic::Ordering::Relaxed);
                        let delta = now.saturating_sub(reported);
                        reported = now;
                        let frame = json!({ "type": "dropped", "count": delta });
                        if send_control(&mut socket, &frame).await.is_err() {
                            break;
                        }
                    }
                    None => {}
                },
                Some(Ok(Message::Ping(data))) => {
                    if socket.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },

            _ = ping.tick() => {
                if send_control(&mut socket, &json!({ "type": "ping" })).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn send_control(socket: &mut WebSocket, frame: &serde_json::Value) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(Utf8Bytes::from(frame.to_string())))
        .await
}

enum ClientControl {
    Pause,
    Resume,
}

fn client_control(text: &str) -> Option<ClientControl> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    match value.get("type")?.as_str()? {
        "pause" => Some(ClientControl::Pause),
        "resume" => Some(ClientControl::Resume),
        _ => None,
    }
}
