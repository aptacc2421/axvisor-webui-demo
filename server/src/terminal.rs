//! ws 终端传输层（对应 webui 的串口终端桥）。
//!
//! 两条路由共用同一套握手与帧协议：
//!   GET /ws/term?token=            全局终端（step-2，hello task 的订阅位）
//!   GET /ws/vms/{id}/console?token=  第 id 台 VM 的 console（step-6，每 VM 独占）
//!
//! 三件事：独占订阅（第二连接 409）、帧协议 v1（Binary=数据 / Text=控制）、
//! 背压演示（暂停时停止消费，让通道积压溢出，生产者只丢不堵）。

use std::{future::Future, time::Duration};

use axum::{
    body::Bytes,
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        FromRequestParts, Path, Query, Request, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    state::{AppState, Evidence, HelloSubscription, SubErr},
    TOKEN,
};

/// 有数据流量时重置计时，所以空闲才 ping。
const PING_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// 订阅位的抢占结果：空闲 / 被占 / 资源不存在。
enum Gate {
    Free,
    Busy,
    NotFound,
}

pub async fn ws_term(
    q: Query<TokenQuery>,
    state: State<AppState>,
    req: Request,
) -> Response {
    let gate = async {
        if state.hello.is_busy().await {
            Gate::Busy
        } else {
            Gate::Free
        }
    };
    let subscribe = async {
        state
            .hello
            .subscribe()
            .await
            .ok_or(Gate::Busy)
    };
    ws_common(q, &state, req, gate, subscribe, "term").await
}

/// 浏览器 ws 发不了自定义 header，所以 token 走查询参数（§4.3）。
/// 每 VM 独占：一台 VM 的 console 只有一个订阅位。
pub async fn ws_vm_console(
    q: Query<TokenQuery>,
    state: State<AppState>,
    Path(id): Path<u64>,
    req: Request,
) -> Response {
    let gate = async {
        match state.vms.is_busy(id).await {
            Some(true) => Gate::Busy,
            Some(false) => Gate::Free,
            None => Gate::NotFound,
        }
    };
    let subscribe = async {
        state.vms.subscribe(id).await.map_err(|e| match e {
            SubErr::Busy => Gate::Busy,
            SubErr::NotFound => Gate::NotFound,
        })
    };
    let tag = format!("vm{id}");
    ws_common(q, &state, req, gate, subscribe, &tag).await
}

/// 共同的握手次序（两路由一致）：
/// token 校验 → 订阅位检查（upgrade 之前，被占直接 409）→
/// 无 Upgrade 头 = 探测请求（浏览器 ws 拿不到握手状态码，只能先 fetch 问一次）
/// → 真正订阅（此处才占订阅位）→ upgrade。
async fn ws_common(
    q: Query<TokenQuery>,
    state: &AppState,
    req: Request,
    gate: impl Future<Output = Gate>,
    subscribe: impl Future<Output = Result<HelloSubscription, Gate>>,
    tag: &str,
) -> Response {
    if q.token.as_deref() != Some(TOKEN) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    match gate.await {
        Gate::Busy => return busy(),
        Gate::NotFound => return not_found(),
        Gate::Free => {}
    }

    let (mut parts, _body) = req.into_parts();

    if !is_websocket_upgrade(&parts.headers) {
        return upgrade_required();
    }

    let sub = match subscribe.await {
        Ok(sub) => sub,
        // Gate::Free 不会从 subscribe 里出来，这里与 Busy 同路返回
        Err(Gate::Free) | Err(Gate::Busy) => return busy(),
        Err(Gate::NotFound) => return not_found(),
    };

    let evidence = state.evidence.clone();
    let tag = tag.to_string();
    match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws.on_upgrade(move |socket| session(socket, sub, evidence, tag)),
        Err(_) => upgrade_required(),
    }
}

async fn session(
    mut socket: WebSocket,
    mut sub: HelloSubscription,
    evidence: Evidence,
    tag: String,
) {
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
                    evidence.console(&tag, "OUT", &line);
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
                    evidence.console(&tag, "IN", &format!("{text:?}"));
                    if socket.send(Message::Binary(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => match client_control(&text) {
                    Some(ClientControl::Pause) => {
                        evidence.console(&tag, "CTL", "pause");
                        paused = true;
                    }
                    Some(ClientControl::Resume) => {
                        evidence.console(&tag, "CTL", "resume");
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

fn busy() -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": "terminal busy" })),
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "not found" })),
    )
        .into_response()
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

async fn send_control(
    socket: &mut WebSocket,
    frame: &serde_json::Value,
) -> Result<(), axum::Error> {
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
