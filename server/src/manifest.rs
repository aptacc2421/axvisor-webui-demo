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
            // counter 回归本位：独立资源演示（reset 的 async + 轮询语义）。
            // step-3 拔过它——重插 = 加回一个 JSON 节点，面板代码一直在。
            PanelMeta {
                kind: "counter",
                title: "计数器",
                verbs: vec!["read", "write"],
            },
            // step-2 挂上的功能 B：全局终端
            PanelMeta {
                kind: "terminal",
                title: "终端",
                verbs: vec!["read", "write", "stream"],
            },
            // step-4 引入、step-6 资源化的组合面板：
            // VM 列表 + 生命周期 + 每 VM console 子页签
            PanelMeta {
                kind: "vm",
                title: "虚拟机",
                verbs: vec!["read", "write", "stream"],
            },
        ],
    })
}
