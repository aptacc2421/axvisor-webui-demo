//! GET /ws/vms/{id}/console?token= —— VM console（模拟 shell 终端）。
//!
//! 三件事：独占订阅（第二连接 409）、行式协议（Binary=命令行/输出，
//! Text=控制帧 ping）、回执可验证（命令输出即执行证据；落盘另见 console.log）。

use std::time::Duration;

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
    state::{AppState, ConsoleSession},
    TOKEN,
};

/// 空闲才 ping。
const PING_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// 浏览器 ws 发不了自定义 header，所以 token 走查询参数（§4.3）。
pub async fn ws_vm_console(
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
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
    if state.vms.is_busy(id).await == Some(true) {
        return busy();
    }
    if state.vms.is_busy(id).await.is_none() {
        return not_found();
    }

    let (mut parts, _body) = req.into_parts();

    // 没有 Upgrade 头的普通 GET 是前端的探测请求——浏览器 WebSocket 拿不到握手
    // 状态码，只能先用 fetch 问一次「订阅位空着吗」。走到这里说明是空的。
    if !is_websocket_upgrade(&parts.headers) {
        return upgrade_required();
    }

    let Ok(sub) = state.vms.subscribe(id).await else {
        return busy();
    };

    let evidence = state.evidence.clone();
    match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws.on_upgrade(move |socket| session(socket, sub, evidence)),
        Err(_) => upgrade_required(),
    }
}

async fn session(
    mut socket: WebSocket,
    sub: ConsoleSession,
    evidence: crate::state::Evidence,
) {
    let tag = format!("vm{}", sub.vm_id);

    // 欢迎横幅 + 首个提示符
    let banner = format!(
        "connected to vm{} console (simulated shell)\r\n\
         commands: pwd | ls | mkdir <dir> | cd <path> | echo <text> [> file] | cat <file>\r\n",
        sub.vm_id
    );
    let prompt = sub.shell.lock().unwrap().prompt(sub.vm_id);
    let first = format!("{banner}{prompt}");
    evidence.console(&tag, "OUT", &first);
    if socket
        .send(Message::Binary(Bytes::from(first)))
        .await
        .is_err()
    {
        return;
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();

    loop {
        tokio::select! {
            msg = socket.recv() => match msg {
                Some(Ok(msg)) => {
                    // 行式协议：一帧一条命令（Text/Binary 均可——不同 ws 客户端
                    // 的字符串发送实现不一）。输出由 shell 执行产生（真实系统里
                    // 由 guest 串口产生，demo 由 server 代演，§7）
                    let line = match &msg {
                        Message::Text(text) => Some(text.trim_end().to_string()),
                        Message::Binary(data) => {
                            Some(String::from_utf8_lossy(data).trim_end().to_string())
                        }
                        _ => None,
                    };
                    if let Some(line) = line {
                        evidence.console(&tag, "IN", &line);

                        let output = sub.shell.lock().unwrap().execute(&line);
                        let prompt = sub.shell.lock().unwrap().prompt(sub.vm_id);
                        let full = if output.is_empty() {
                            prompt
                        } else {
                            format!("{output}\r\n{prompt}")
                        };
                        evidence.console(&tag, "OUT", &full);
                        if socket
                            .send(Message::Binary(Bytes::from(full)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }

                    match msg {
                        Message::Ping(data) => {
                            if socket.send(Message::Pong(data)).await.is_err() {
                                break;
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                Some(Err(_)) | None => break,
            },

            _ = ping.tick() => {
                let frame = json!({ "type": "ping" });
                if socket
                    .send(Message::Text(Utf8Bytes::from(frame.to_string())))
                    .await
                    .is_err()
                {
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

pub(crate) fn upgrade_required() -> Response {
    (
        StatusCode::UPGRADE_REQUIRED,
        Json(json!({ "error": "upgrade required" })),
    )
        .into_response()
}

pub(crate) fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
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
