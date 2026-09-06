//! 执行层句柄的持有者。
//!
//! bare 阶段：没有任何资源端点——只有静态资产目录与一个恒空的资源事件
//! 广播通道（/ws/events 连上即得一帧空快照，此后保持连接、无变更）。
//! 随 step 演化：with-vms 起这里长出 VmManager。

use std::path::PathBuf;
use tokio::sync::watch;

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    /// 资源事件广播端：bare 阶段恒为空列表；持住 sender 使订阅端
    /// 收到初始快照后保持连接（不再有变更 → yield）。
    events_tx: watch::Sender<Vec<serde_json::Value>>,
}

impl AppState {
    pub fn new() -> Self {
        let (events_tx, _events_rx) = watch::channel(Vec::new());
        Self {
            dist: dist_dir(),
            events_tx,
        }
    }

    /// 资源事件订阅：/ws/events 端点用它做 wake/yield。
    pub fn resource_events(&self) -> watch::Receiver<Vec<serde_json::Value>> {
        self.events_tx.subscribe()
    }
}

fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}
