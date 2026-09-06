//! GET /api/manifest —— 资源层发现。
//!
//! 前端导航 100% 由这里的数据生成（不变量 5）：后端挂上 = 界面出现。

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct PanelMeta {
    pub kind: &'static str,
    pub title: &'static str,
    pub verbs: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct Manifest {
    pub proto: u8,
    pub panels: Vec<PanelMeta>,
}

pub async fn get_manifest() -> Json<Manifest> {
    Json(Manifest {
        proto: 1,
        panels: vec![
            // step-3 拔出演示：counter 节点从这里删掉——面板代码、路由、执行层
            // 全都还在，拔的只是「UI 挂载」。POST /api/counter 照常 200。
            PanelMeta {
                kind: "terminal",
                title: "终端",
                verbs: vec!["read", "write", "stream"],
            },
        ],
    })
}
