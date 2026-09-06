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
            // step-1 挂上的功能 A——后端 manifest 加一个节点，前端导航就多一项
            PanelMeta {
                kind: "counter",
                title: "计数器",
                verbs: vec!["read", "write"],
            },
            // step-2 挂上的功能 B。
            // probe 在这里退役：它是 step-0 演示降级用的，那时还没有任何渲染器；
            // 现在降级路径已经由 counter/terminal 之外「未知的 kind」继续保证。
            PanelMeta {
                kind: "terminal",
                title: "终端",
                verbs: vec!["read", "write", "stream"],
            },
        ],
    })
}
