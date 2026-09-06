//! 执行层句柄的持有者。
//!
//! with-vms 阶段：长出 VmManager——VM 资源（创建/停止/列表）与资源事件广播。
//! 终端（console 订阅与模拟 shell）在 full 分支才出现。

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    pub vms: VmManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            dist: dist_dir(),
            vms: spawn_vm_manager(),
        }
    }

    /// 资源事件订阅：/ws/events 端点用它做 wake/yield。
    pub fn resource_events(&self) -> watch::Receiver<Vec<(u64, VmState)>> {
        self.vms.events()
    }
}

fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

// ── 演示证据：执行层状态落盘 ────────────────────────────────────────────────
//
// UI 会骗人，文件不会。VM 生命周期写进 server/data/vms.log 与 vms.json，
// 「stop 真的执行了」用 cat / tail -f 独立验证。
// （axvisor 侧无 fs，走它自己的机制，这是 §7 平台差异在写路径上的对应物。）

#[derive(Clone)]
pub struct Evidence {
    dir: PathBuf,
}

impl Evidence {
    pub fn new() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("[evidence] 建目录失败: {e}");
        }
        Self { dir }
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// VM 生命周期事件追加进 vms.log：CREATE / STOP / STOPPED
    pub fn vm_event(&self, event: &str, id: u64) {
        let line = format!("[{}] {} vm{}\n", Self::now(), event, id);
        let path = self.dir.join("vms.log");
        let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
            eprintln!("[evidence] 打开 vms.log 失败");
            return;
        };
        if let Err(e) = file.write_all(line.as_bytes()) {
            eprintln!("[evidence] 写 vms.log 失败: {e}");
        }
    }

    /// VM 列表快照：每次变化重写 vms.json
    pub fn vm_snapshot(&self, vms: &[(u64, VmState)]) {
        let body = serde_json::json!({
            "updated_at_unix": Self::now(),
            "vms": vms.iter().map(|(id, s)| serde_json::json!({
                "id": id, "state": s.as_str(),
            })).collect::<Vec<_>>(),
        });
        if let Err(e) = fs::write(self.dir.join("vms.json"), body.to_string()) {
            eprintln!("[evidence] 写 vms.json 失败: {e}");
        }
    }
}

// ── 执行层：VmManager（VM 资源化）──────────────────────────────────────────
//
// VM 是资源。stop 是异步的：接受后 2s 收尾（state → stopped）。
// 资源变更 send_replace 进 watch（wake），/ws/events 挂起等变化（yield）。

const VM_STOP_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq)]
pub enum VmState {
    Running,
    Stopping,
    Stopped,
}

impl VmState {
    pub fn as_str(&self) -> &'static str {
        match self {
            VmState::Running => "running",
            VmState::Stopping => "stopping",
            VmState::Stopped => "stopped",
        }
    }
}

enum VmCmd {
    Create(oneshot::Sender<u64>),
    /// 回执 false = 不存在或已停止
    Stop(u64, oneshot::Sender<bool>),
    /// 延迟任务收尾：stopping → stopped
    FinishStop(u64),
}

#[derive(Clone)]
pub struct VmManager {
    tx: mpsc::Sender<VmCmd>,
    /// 资源列表快照的广播端：每次变更 send_replace（wake）。
    state_tx: watch::Sender<Vec<(u64, VmState)>>,
}

impl VmManager {
    /// 资源列表快照：直接读 watch 里的当前值（O(1)，不经命令通道）。
    pub fn list(&self) -> Vec<(u64, VmState)> {
        self.state_tx.borrow().clone()
    }

    /// 资源事件订阅：/ws/events 端点用它做 wake/yield。
    pub fn events(&self) -> watch::Receiver<Vec<(u64, VmState)>> {
        self.state_tx.subscribe()
    }

    pub async fn create(&self) -> Option<u64> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(VmCmd::Create(reply)).await.ok()?;
        rx.await.ok()
    }

    pub async fn stop(&self, id: u64) -> Option<bool> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(VmCmd::Stop(id, reply)).await.ok()?;
        rx.await.ok()
    }
}

fn spawn_vm_manager() -> VmManager {
    let (tx, mut rx) = mpsc::channel::<VmCmd>(32);
    let self_tx = tx.clone();
    let (state_tx, _state_rx) = watch::channel(Vec::new());
    let loop_tx = state_tx.clone();
    let evidence = Evidence::new();

    tokio::spawn(async move {
        let mut next_id: u64 = 1;
        let mut vms: HashMap<u64, VmState> = HashMap::new();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                VmCmd::Create(reply) => {
                    let id = next_id;
                    next_id += 1;
                    vms.insert(id, VmState::Running);
                    evidence.vm_event("CREATE", id);
                    vm_snapshot(&vms, &evidence, &loop_tx);
                    let _ = reply.send(id);
                }

                VmCmd::Stop(id, reply) => {
                    let accepted = match vms.get_mut(&id) {
                        Some(state @ VmState::Running) => {
                            *state = VmState::Stopping;
                            // 异步语义：先回执，2s 后由延迟任务收尾
                            let mgr_tx = self_tx.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(VM_STOP_DELAY).await;
                                let _ = mgr_tx.send(VmCmd::FinishStop(id)).await;
                            });
                            evidence.vm_event("STOP", id);
                            vm_snapshot(&vms, &evidence, &loop_tx);
                            true
                        }
                        // 重复 stop 幂等接受；已停止/不存在 → false
                        Some(state @ VmState::Stopping) => {
                            let _ = state;
                            true
                        }
                        _ => false,
                    };
                    let _ = reply.send(accepted);
                }

                VmCmd::FinishStop(id) => {
                    if let Some(state @ VmState::Stopping) = vms.get_mut(&id) {
                        *state = VmState::Stopped;
                        evidence.vm_event("STOPPED", id);
                        vm_snapshot(&vms, &evidence, &loop_tx);
                    }
                }
            }
        }
    });

    VmManager { tx, state_tx }
}

fn vm_snapshot(
    vms: &HashMap<u64, VmState>,
    evidence: &Evidence,
    state_tx: &watch::Sender<Vec<(u64, VmState)>>,
) {
    let mut list: Vec<(u64, VmState)> = vms.iter().map(|(id, s)| (*id, *s)).collect();
    list.sort_by_key(|(id, _)| *id);
    evidence.vm_snapshot(&list);
    state_tx.send_replace(list);
}
