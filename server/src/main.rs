//! 唯一 router() 装配点：/api + /ws + / 静态，全部 merge 进一个 Router 实例，
//! 一个 serve（不变量 1）。curl 与浏览器走的是同一棵路由树（不变量 8）。

mod events;
mod manifest;
mod state;
mod vm;

use axum::{
    extract::{Request, State},
    http::header,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use state::AppState;
use tower_http::services::{ServeDir, ServeFile};

/// 固定 token（内存态）。真实系统的 ticket/轮换机制不在 demo 范围内（§8）。
pub const TOKEN: &str = "demo-token";

#[tokio::main]
async fn main() {
    let state = AppState::new();
    let app = router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("webui_demo server listening on http://0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

fn router(state: AppState) -> Router {
    // 静态资产：直读 web/dist（§7 的平台差异）；未构建时 / 返回 503，
    // 但 /api/* 照常可用——后端能力不依赖前端产物。
    let dist = state.dist.clone();
    let static_files = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));

    // 传输层统一鉴权：/api 都要过这一层（/ws 走查询参数，由 handler 自校验）。
    let authed = Router::new()
        .route("/api/manifest", get(manifest::get_manifest))
        .route("/api/vms", get(vm::list_vms).post(vm::post_vms))
        .route("/api/vms/{id}/stop", post(vm::post_vm_stop))
        .layer(middleware::from_fn(require_token))
        .with_state(state.clone());

    Router::new()
        .merge(authed)
        // 壳级资源事件流：token 走查询参数，自带鉴权
        .route("/ws/events", get(events::ws_events))
        // /api、/ws 下没匹配到的路径给 JSON 404，而不是落进 SPA 兜底返回 HTML——
        // curl 是一等公民（不变量 8），它拿到的响应得是接口形状。
        .route("/api/{*path}", any(api_not_found))
        .route("/ws/{*path}", any(api_not_found))
        .route("/", get(index))
        .fallback_service(static_files)
        .with_state(state)
}

async fn api_not_found() -> Response {
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({ "error": "not found" })),
    )
        .into_response()
}

async fn require_token(req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim() == TOKEN)
        .unwrap_or(false);

    if !ok {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    next.run(req).await
}

async fn index(State(state): State<AppState>) -> Response {
    let file = state.dist.join("index.html");
    if !file.exists() {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "web/dist 未构建，请先执行：cd web && npm run build",
        )
            .into_response();
    }
    match tokio::fs::read(&file).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
