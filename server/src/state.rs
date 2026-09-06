//! 执行层句柄的持有者。
//!
//! HTTP 层（events.rs / terminal.rs / vm.rs）永远只通过这里暴露的句柄与执行层
//! 通信，不直捣执行层状态（不变量 2）。本文件随 step 演化：
//!   step-6: + VmManager（VM 资源）
//!   step-10: hello 流与 counter 退役，每 VM 改持一个模拟 shell

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::shell::Shell;

/// 订阅失败的两种原因：订阅位被占 / 资源不存在。
pub enum SubErr {
    Busy,
    NotFound,
}

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    pub vms: VmManager,
    pub evidence: Evidence,
}

impl AppState {
    pub fn new() -> Self {
        let evidence = Evidence::new();
        Self {
            dist: dist_dir(),
            vms: spawn_vm_manager(evidence.clone()),
            evidence,
        }
    }
}

fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

// ── 演示证据：执行层状态落盘 ────────────────────────────────────────────────
//
// UI 会骗人，文件不会。终端收发、VM 生命周期都写进 server/data/，
// 「输入真的到了」「stop 真的执行了」用 cat / tail -f 独立验证。
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

    /// 终端会话收发追加进 console.log：IN = 用户命令，OUT = 执行输出/提示符。
    /// tag 区分通道：vm{id} = 第 id 台 VM 的 console。
    pub fn console(&self, tag: &str, direction: &str, text: &str) {
        let text = text.trim().replace('\n', "\\n");
        let line = format!("[{}] {tag} {direction} {text}\n", Self::now());
        let path = self.dir.join("console.log");
        let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
            eprintln!("[evidence] 打开 console.log 失败");
            return;
        };
        if let Err(e) = file.write_all(line.as_bytes()) {
            eprintln!("[evidence] 写 console.log 失败: {e}");
        }
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
// VM 是资源。每台 VM 一份模拟 shell（内存文件系统 + 工作目录）；
// console 独占订阅位靠「座位通道 receiver 的单一所有权」实现——
// 会话断开 receiver 被 drop，订阅位自动释放。
// stop 是异步的：接受后 2s 收尾（state → stopped），页签不消失。

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

/// 一次 console 订阅：shell 句柄 + 独占座位。
pub struct ConsoleSession {
    pub vm_id: u64,
    pub shell: Arc<Mutex<Shell>>,
    /// 座位通道：receiver 活着 = 订阅位被占（通道本身不运载数据）
    _seat_rx: mpsc::Receiver<()>,
}

enum VmCmd {
    Create(oneshot::Sender<u64>),
    /// 回执 false = 不存在或已停止
    Stop(u64, oneshot::Sender<bool>),
    /// 延迟任务收尾：stopping → stopped
    FinishStop(u64),
    /// None = VM 不存在
    IsBusy(u64, oneshot::Sender<Option<bool>>),
    Subscribe(u64, oneshot::Sender<Result<ConsoleSession, SubErr>>),
}

#[derive(Clone)]
pub struct VmManager {
    tx: mpsc::Sender<VmCmd>,
    /// 资源列表快照的广播端：每次变更 send_replace（wake），
    /// /ws/events 订阅端挂着等变化（yield）——事件驱动的核心通道。
    state_tx: watch::Sender<Vec<(u64, VmState)>>,
}

struct VmEntry {
    state: VmState,
    shell: Arc<Mutex<Shell>>,
    /// 独占座位：None-语义不需要——订阅时换新 tx 交给会话；
    /// 会话断开 → tx.is_closed() → 订阅位释放。
    seat: Option<mpsc::Sender<()>>,
}

impl VmManager {
    /// 资源列表快照：直接读 watch 里的当前值（O(1)，不经命令通道）。
    pub fn list(&self) -> Vec<(u64, VmState)> {
        self.state_tx.borrow().clone()
    }

    /// 资源事件订阅：/ws/events 端点用它挂起等变更（wake/yield）。
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

    /// None = VM 不存在
    pub async fn is_busy(&self, id: u64) -> Option<bool> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(VmCmd::IsBusy(id, reply)).await.ok()?;
        rx.await.ok().flatten()
    }

    pub async fn subscribe(&self, id: u64) -> Result<ConsoleSession, SubErr> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(VmCmd::Subscribe(id, reply)).await.is_err() {
            return Err(SubErr::NotFound);
        }
        rx.await.unwrap_or(Err(SubErr::Busy))
    }
}

fn spawn_vm_manager(evidence: Evidence) -> VmManager {
    let (tx, mut rx) = mpsc::channel::<VmCmd>(32);
    let self_tx = tx.clone();
    let (state_tx, _state_rx) = watch::channel(Vec::new());
    let loop_tx = state_tx.clone();

    tokio::spawn(async move {
        let mut next_id: u64 = 1;
        let mut vms: HashMap<u64, VmEntry> = HashMap::new();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                VmCmd::Create(reply) => {
                    let id = next_id;
                    next_id += 1;
                    let (seat_tx, _seat_rx) = mpsc::channel::<()>(1);
                    vms.insert(
                        id,
                        VmEntry {
                            state: VmState::Running,
                            shell: Arc::new(Mutex::new(Shell::new())),
                            seat: Some(seat_tx),
                        },
                    );
                    evidence.vm_event("CREATE", id);
                    vm_snapshot(&vms, &evidence, &loop_tx);
                    let _ = reply.send(id);
                }

                VmCmd::Stop(id, reply) => {
                    let accepted = match vms.get_mut(&id) {
                        Some(e) if e.state == VmState::Running => {
                            e.state = VmState::Stopping;
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
                        Some(e) if e.state == VmState::Stopping => true,
                        _ => false,
                    };
                    let _ = reply.send(accepted);
                }

                VmCmd::FinishStop(id) => {
                    if let Some(e) = vms.get_mut(&id) {
                        if e.state == VmState::Stopping {
                            e.state = VmState::Stopped;
                            evidence.vm_event("STOPPED", id);
                            vm_snapshot(&vms, &evidence, &loop_tx);
                        }
                    }
                }

                VmCmd::IsBusy(id, reply) => {
                    let busy = vms
                        .get(&id)
                        .map(|e| e.seat.as_ref().is_some_and(|tx| !tx.is_closed()));
                    let _ = reply.send(busy);
                }

                VmCmd::Subscribe(id, reply) => match vms.get_mut(&id) {
                    Some(e) => {
                        if e.seat.as_ref().is_some_and(|tx| !tx.is_closed()) {
                            let _ = reply.send(Err(SubErr::Busy));
                        } else {
                            // 新座位：订阅 = 接管；旧 receiver 若已被 drop 则座位已空
                            let (seat_tx, seat_rx) = mpsc::channel::<()>(1);
                            e.seat = Some(seat_tx);
                            let _ = reply.send(Ok(ConsoleSession {
                                vm_id: id,
                                shell: Arc::clone(&e.shell),
                                _seat_rx: seat_rx,
                            }));
                        }
                    }
                    None => {
                        let _ = reply.send(Err(SubErr::NotFound));
                    }
                },
            }
        }
    });

    VmManager { tx, state_tx }
}

fn vm_snapshot(
    vms: &HashMap<u64, VmEntry>,
    evidence: &Evidence,
    state_tx: &watch::Sender<Vec<(u64, VmState)>>,
) {
    let mut list: Vec<(u64, VmState)> = vms.iter().map(|(id, e)| (*id, e.state)).collect();
    list.sort_by_key(|(id, _)| *id);
    evidence.vm_snapshot(&list);
    state_tx.send_replace(list);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_per_vm_is_isolated() {
        let mut a = Shell::new();
        let mut b = Shell::new();
        a.execute("mkdir only-a");
        assert_eq!(a.execute("cd only-a"), "");
        assert!(b.execute("cd only-a").contains("No such file"));
    }
}
