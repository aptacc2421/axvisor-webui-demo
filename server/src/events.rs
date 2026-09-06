//! GET /ws/events?token= —— 壳级资源事件流（WebSocket，无独占）。
//!
//! 传输选型对齐 axvisor 真身（v3 设计文档「传输选型」：WebSocket；
//! SSE 因单向下行被真身否决，不用）。
//! bare 阶段：没有任何资源端点——连接即得一帧空快照，此后保持连接
//! （心跳），直到本分支长出资源端点开始 wake/yield。

use std::time::Duration;

use axum::{
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

use crate::{state::AppState, TOKEN};

const PING_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

pub async fn ws_events(
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

    let (mut parts, _body) = req.into_parts();
    if !is_websocket_upgrade(&parts.headers) {
        return upgrade_required();
    }

    let rx = state.resource_events();
    match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws.on_upgrade(move |socket| session(socket, rx)),
        Err(_) => upgrade_required(),
    }
}

async fn session(
    mut socket: WebSocket,
    mut rx: tokio::sync::watch::Receiver<Vec<serde_json::Value>>,
) {
    // 连接即得一帧当前快照（bare 恒为空），此后无变更 → 挂起保持连接
    let list = rx.borrow().clone();
    if push(&mut socket, &list).await.is_err() {
        return;
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();
    loop {
        tokio::select! {
            Ok(_) = rx.changed() => {
                let list = rx.borrow().clone();
                if push(&mut socket, &list).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if send(&mut socket, &json!({ "type": "ping" })).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn push(socket: &mut WebSocket, list: &[serde_json::Value]) -> Result<(), axum::Error> {
    send(socket, &json!({ "type": "vms", "vms": list })).await
}

async fn send(socket: &mut WebSocket, frame: &serde_json::Value) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(Utf8Bytes::from(frame.to_string())))
        .await
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
