//! GET /ws/events?token= —— 壳级资源事件流（WebSocket，无独占）。
//!
//! 传输选型对齐 axvisor 真身（v3 设计文档「传输选型」：WebSocket；
//! SSE 因单向下行被真身否决，不用）。
//!
//! wake/yield：VmManager 每次资源变更 send_replace 进 watch 通道，
//! 本会话挂起等变化——有变更就 wake 推一帧，没有就 yield。
//! 帧协议 v1：Text=控制/状态帧（hello / vms / ping），本通道无 Binary。

use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        FromRequestParts, Query, Request, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

use crate::{
    state::{AppState, VmState},
    terminal::{is_websocket_upgrade, upgrade_required},
    TOKEN,
};

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

    let rx = state.vms.events();
    match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws.on_upgrade(move |socket| session(socket, rx)),
        Err(_) => upgrade_required(),
    }
}

async fn session(mut socket: WebSocket, mut rx: watch::Receiver<Vec<(u64, VmState)>>) {
    // 帧协议 v1：先 hello，再发当前快照——客户端连上即得全量状态
    if send(&mut socket, &json!({ "type": "hello", "proto": 1 })).await.is_err() {
        return;
    }
    let snapshot = rx.borrow().clone();
    if push(&mut socket, &snapshot).await.is_err() {
        return;
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();

    loop {
        tokio::select! {
            // wake/yield：没变更就挂在这里，不来轮询
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

async fn send(socket: &mut WebSocket, frame: &serde_json::Value) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(Utf8Bytes::from(frame.to_string())))
        .await
}

async fn push(socket: &mut WebSocket, list: &[(u64, VmState)]) -> Result<(), axum::Error> {
    let vms = list
        .iter()
        .map(|(id, s)| json!({ "id": id, "state": s.as_str() }))
        .collect::<Vec<_>>();
    send(socket, &json!({ "type": "vms", "vms": vms })).await
}
