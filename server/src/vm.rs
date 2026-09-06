//! GET/POST /api/vms —— VM 资源化（对应真实 webui 的 VM 列表与生命周期）。
//!
//! - GET  /api/vms            → {"vms":[{id,state},...]}
//! - POST /api/vms            → {"action":"create"}，同步返回新 id（对应轻量操作）
//! - POST /api/vms/{id}/stop  → 异步接受，2s 后停产（对应 start/stop：
//!   返回 async，前端轮询到终态；页签不消失，console 只是安静了）
//!
//! handler 仍是薄壳：VM 的状态只被 VmManager task 拥有（不变量 2）。

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;

#[derive(Deserialize)]
struct VmRequest {
    action: String,
}

pub async fn list_vms(State(state): State<AppState>) -> Json<serde_json::Value> {
    let list = state.vms.list();
    let vms = list
        .iter()
        .map(|(id, s)| json!({ "id": id, "state": s.as_str() }))
        .collect::<Vec<_>>();
    Json(json!({ "vms": vms }))
}

/// body 按字节解析，不校验 Content-Type（curl 一等公民，同 counter）。
pub async fn post_vms(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let req: VmRequest = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    match req.action.as_str() {
        "create" => {
            let id = state
                .vms
                .create()
                .await
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(json!({ "ok": true, "id": id, "state": "running" })).into_response())
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

pub async fn post_vm_stop(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let req: VmRequest = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if req.action != "stop" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let accepted = state
        .vms
        .stop(id)
        .await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    if !accepted {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not found" })),
        )
            .into_response());
    }
    Ok(Json(json!({
        "ok": true,
        "async": true,
        "id": id,
        "status": "stopping",
    }))
    .into_response())
}
