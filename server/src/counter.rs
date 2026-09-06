//! GET/POST /api/counter —— 对应 webui 的 VM 生命周期控制面。
//!
//! handler 是薄壳：它不认识值，只知道往执行层发命令（不变量 2）。
//! 三种操作：
//!   GET              → 当前值
//!   POST {delta}     → 同步返回新值（对应轻量操作）
//!   POST {reset}     → 异步接受，2s 后归零（对应 start/stop：返回 async，前端轮询）

use std::time::Duration;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// 复位延迟：故意做成看得见的 2s，让「async + 轮询到终态」在界面上可观察。
const RESET_DELAY: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
pub struct CounterRequest {
    pub delta: Option<i64>,
    pub reset: Option<bool>,
}

#[derive(Serialize)]
pub struct CounterValue {
    pub value: i64,
}

#[derive(Serialize)]
pub struct AsyncAccepted {
    pub ok: bool,
    pub r#async: bool,
    pub status: &'static str,
}

pub async fn get_counter(State(state): State<AppState>) -> Result<Json<CounterValue>, StatusCode> {
    let value = state
        .counter
        .get()
        .await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(CounterValue { value }))
}

/// body 按字节解析，不校验 Content-Type。
///
/// curl 是一等公民（不变量 8），而 `curl -d '{"delta":1}'` 发的是
/// application/x-www-form-urlencoded——若用 axum 的 Json 提取器会直接 415，
/// SPEC §6 的验收命令就跑不通。demo 的边界只有一个 POST，按内容解析即可。
pub async fn post_counter(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let req: CounterRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    if req.reset == Some(true) {
        state
            .counter
            .reset(RESET_DELAY)
            .await
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(AsyncAccepted {
            ok: true,
            r#async: true,
            status: "resetting",
        })
        .into_response());
    }

    let delta = req.delta.unwrap_or(0);
    let value = state
        .counter
        .add(delta)
        .await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(CounterValue { value }).into_response())
}
