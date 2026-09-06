//! 执行层句柄的持有者。
//!
//! HTTP 层（counter.rs / terminal.rs）永远只通过这里暴露的句柄与执行层通信，
//! 不直捣执行层状态（不变量 2）。本文件随 step 增长：
//!   step-0: 只有静态资产目录
//!   step-1: + counter task 的命令通道
//!   step-2: + hello task 的订阅句柄

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::sync::mpsc::error::TrySendError;

/// 前端构建产物目录。Linux 有 page cache，直读 fs 即「按需缺页」（§7）。
fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

/// 订阅失败的两种原因：订阅位被占 / 资源不存在。
pub enum SubErr {
    Busy,
    NotFound,
}

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    pub counter: CounterHandle,
    pub hello: HelloHandle,
    pub vms: VmManager,
    pub evidence: Evidence,
}

impl AppState {
    pub fn new() -> Self {
        let evidence = Evidence::new();
        Self {
            dist: dist_dir(),
            counter: spawn_counter(evidence.clone()),
            hello: spawn_hello(),
            vms: spawn_vm_manager(evidence.clone()),
            evidence,
        }
    }
}

// ── 演示证据：执行层状态落盘 ────────────────────────────────────────────────
//
// UI 会骗人，文件不会。counter 每次变化重写 counter.json，终端会话把每一行
// 收发追加进 console.log——「计数真的变了」「输入真的到了」用 cat / tail -f
// 独立验证，不必信浏览器。（axvisor 侧无 fs，走它自己的机制，这是 §7 的
// 平台差异在写路径上的对应物。）

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

    /// counter 每次变化就重写 counter.json
    pub fn counter(&self, value: i64) {
        let body = serde_json::json!({ "value": value, "updated_at_unix": Self::now() });
        if let Err(e) = fs::write(self.dir.join("counter.json"), body.to_string()) {
            eprintln!("[evidence] 写 counter.json 失败: {e}");
        }
    }

    /// 终端会话收发追加进 console.log：IN = 用户输入，OUT = 下行数据。
    /// tag 区分通道：term = 全局终端，vm{id} = 第 id 台 VM 的 console。
    pub fn console(&self, tag: &str, direction: &str, text: &str) {
        let line = format!("[{}] {tag} {direction} {}\n", Self::now(), text);
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

// ── 执行层：counter task ────────────────────────────────────────────────────
//
// 值只被这个 task 独占拥有（不变量 2）。HTTP handler 想读写它，只能经命令通道
// 发一个带 oneshot 回执的请求，绝不直捣。

enum CounterCmd {
    Get(oneshot::Sender<i64>),
    Add(i64, oneshot::Sender<i64>),
    Reset(Duration, oneshot::Sender<()>),
    CommitReset,
}

#[derive(Clone)]
pub struct CounterHandle {
    tx: mpsc::Sender<CounterCmd>,
}

impl CounterHandle {
    pub async fn get(&self) -> Option<i64> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CounterCmd::Get(reply)).await.ok()?;
        rx.await.ok()
    }

    pub async fn add(&self, delta: i64) -> Option<i64> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CounterCmd::Add(delta, reply)).await.ok()?;
        rx.await.ok()
    }

    /// 异步接受：只保证「复位请求已被接受」，归零发生在 delay 之后。
    pub async fn reset(&self, delay: Duration) -> Option<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(CounterCmd::Reset(delay, reply)).await.ok()?;
        rx.await.ok()
    }
}

fn spawn_counter(evidence: Evidence) -> CounterHandle {
    let (tx, mut rx) = mpsc::channel::<CounterCmd>(32);
    // reset 用的延迟提交通道：执行层自己给自己发命令，HTTP 层不参与
    let commit_tx = tx.clone();
    // 启动即落盘：文件从第一刻起就是真相的来源
    evidence.counter(0);

    tokio::spawn(async move {
        let mut value: i64 = 0;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                CounterCmd::Get(reply) => {
                    let _ = reply.send(value);
                }
                CounterCmd::Add(delta, reply) => {
                    value += delta;
                    evidence.counter(value);
                    let _ = reply.send(value);
                }
                CounterCmd::Reset(delay, reply) => {
                    // 先回执（HTTP 层据此立刻返回 async），延迟由执行层自己收尾。
                    // 期间 task 继续服务 Get/Add——所以前端轮询看得到「还没归零」。
                    let _ = reply.send(());
                    let tx = commit_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let _ = tx.send(CounterCmd::CommitReset).await;
                    });
                }
                CounterCmd::CommitReset => {
                    value = 0;
                    evidence.counter(0);
                }
            }
        }
    });

    CounterHandle { tx }
}

// ── 执行层：hello task（数据面）──────────────────────────────────────────────
//
// 固定节奏生产一行，try_send 进有界通道：满则丢弃并计数，永不阻塞、永不 await
// 重试（不变量 3）。真实系统里这条路径是 guest 串口输出，反压它会拖死 guest。

/// 通道容量。
///
/// §4.3 写的是 32，但那与 §6「暂停 5s 后恢复，丢帧数 > 0」不相容：
/// 32 × 500ms = 16s 才填满，5s 时丢帧必为 0。按 §9.4 记录冲突，取 §6 的
/// 可观察行为为准，容量定为 8（8 × 500ms = 4s 开始丢帧）。改这一个常量即可换回去。
const HELLO_CHANNEL_CAPACITY: usize = 8;

const HELLO_INTERVAL: Duration = Duration::from_millis(500);

/// 一次订阅：通道的 receiver 归当前 ws 会话独占所有。
///
/// 独占就是靠这个单一所有权实现的（§7「语义即代码」）：receiver 还在手上，
/// 订阅位就是被占的；会话一断 receiver 被 drop，订阅位自动释放。
pub struct HelloSubscription {
    pub rx: mpsc::Receiver<String>,
    /// 生产者侧的丢帧计数：满了塞不进去就 +1，会话读它来汇报。
    pub dropped: Arc<AtomicU64>,
}

enum HelloCmd {
    IsBusy(oneshot::Sender<bool>),
    Subscribe(oneshot::Sender<Option<HelloSubscription>>),
}

#[derive(Clone)]
pub struct HelloHandle {
    tx: mpsc::Sender<HelloCmd>,
}

impl HelloHandle {
    /// 订阅位是否被占（供 /ws/term 在 upgrade 之前判断 409）。
    pub async fn is_busy(&self) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(HelloCmd::IsBusy(reply)).await.is_err() {
            return true;
        }
        rx.await.unwrap_or(true)
    }

    /// 申请订阅：被占则返回 None。
    pub async fn subscribe(&self) -> Option<HelloSubscription> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(HelloCmd::Subscribe(reply)).await.ok()?;
        rx.await.ok().flatten()
    }
}

fn spawn_hello() -> HelloHandle {
    let (tx, mut rx) = mpsc::channel::<HelloCmd>(8);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HELLO_INTERVAL);
        let mut n: u64 = 0;
        let mut out: Option<mpsc::Sender<String>> = None;
        let mut dropped = Arc::new(AtomicU64::new(0));

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => match cmd {
                    HelloCmd::IsBusy(reply) => {
                        let busy = out.as_ref().is_some_and(|tx| !tx.is_closed());
                        let _ = reply.send(busy);
                    }
                    HelloCmd::Subscribe(reply) => {
                        // 新订阅 = 建新通道从头开始，不做历史回放（§4.3）
                        if out.as_ref().is_some_and(|tx| !tx.is_closed()) {
                            let _ = reply.send(None);
                        } else {
                            let (line_tx, line_rx) = mpsc::channel(HELLO_CHANNEL_CAPACITY);
                            dropped = Arc::new(AtomicU64::new(0));
                            out = Some(line_tx);
                            let _ = reply.send(Some(HelloSubscription {
                                rx: line_rx,
                                dropped: Arc::clone(&dropped),
                            }));
                        }
                    }
                },

                _ = ticker.tick() => {
                    n += 1;
                    let line = format!("hello world #{n}");
                    if let Some(tx) = out.as_ref() {
                        match tx.try_send(line) {
                            Ok(()) => {}
                            // 通道满：丢这一行，只计数。绝不 await 重试。
                            Err(TrySendError::Full(_)) => {
                                dropped.fetch_add(1, Ordering::Relaxed);
                            }
                            // receiver 被 drop（ws 断开）：订阅位释放，hello task 继续跑
                            Err(TrySendError::Closed(_)) => {
                                dropped.fetch_add(1, Ordering::Relaxed);
                                out = None;
                            }
                        }
                    }
                }
            }
        }
    });

    HelloHandle { tx }
}

// ── 执行层：VmManager（VM 资源化）──────────────────────────────────────────
//
// 「计数」不再等于 VM 数：VM 是资源。每台 VM 有自己的 hello 生产者、自己的
// 有界通道、自己的独占订阅位——同一套不变量按实例生效。stop 是异步的：
// 接受后 2s 才停产，页签不消失、console 连接保持、只是不再有输出（停产不停服）。

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
    List(oneshot::Sender<Vec<(u64, VmState)>>),
    Create(oneshot::Sender<u64>),
    /// 回执 false = 不存在或已停止
    Stop(u64, oneshot::Sender<bool>),
    /// 延迟任务收尾：stopping → stopped
    FinishStop(u64),
    /// None = VM 不存在
    IsBusy(u64, oneshot::Sender<Option<bool>>),
    Subscribe(u64, oneshot::Sender<Result<HelloSubscription, SubErr>>),
}

#[derive(Clone)]
pub struct VmManager {
    tx: mpsc::Sender<VmCmd>,
}

struct VmEntry {
    state: VmState,
    cmd_tx: mpsc::Sender<VmProducerCmd>,
    stop_tx: watch::Sender<bool>,
}

enum VmProducerCmd {
    IsBusy(oneshot::Sender<bool>),
    Subscribe(oneshot::Sender<Result<HelloSubscription, SubErr>>),
}

impl VmManager {
    pub async fn list(&self) -> Option<Vec<(u64, VmState)>> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(VmCmd::List(reply)).await.ok()?;
        rx.await.ok()
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

    pub async fn subscribe(&self, id: u64) -> Result<HelloSubscription, SubErr> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(VmCmd::Subscribe(id, reply)).await.is_err() {
            return Err(SubErr::NotFound);
        }
        // 若 VM 在订阅中途被删（demo 里不会），保守按 Busy 处理
        rx.await.unwrap_or(Err(SubErr::Busy))
    }
}

fn spawn_vm_manager(evidence: Evidence) -> VmManager {
    let (tx, mut rx) = mpsc::channel::<VmCmd>(32);
    let self_tx = tx.clone();

    tokio::spawn(async move {
        let mut next_id: u64 = 1;
        let mut vms: HashMap<u64, VmEntry> = HashMap::new();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                VmCmd::List(reply) => {
                    let mut list: Vec<(u64, VmState)> =
                        vms.iter().map(|(id, e)| (*id, e.state)).collect();
                    list.sort_by_key(|(id, _)| *id);
                    let _ = reply.send(list);
                }

                VmCmd::Create(reply) => {
                    let id = next_id;
                    next_id += 1;
                    let (cmd_tx, cmd_rx) = mpsc::channel::<VmProducerCmd>(8);
                    let (stop_tx, stop_rx) = watch::channel(false);
                    spawn_vm_producer(id, cmd_rx, stop_rx);
                    vms.insert(
                        id,
                        VmEntry {
                            state: VmState::Running,
                            cmd_tx,
                            stop_tx,
                        },
                    );
                    evidence.vm_event("CREATE", id);
                    vm_snapshot(&vms, &evidence);
                    let _ = reply.send(id);
                }

                VmCmd::Stop(id, reply) => {
                    let accepted = match vms.get_mut(&id) {
                        Some(e) if e.state == VmState::Running => {
                            e.state = VmState::Stopping;
                            // 异步语义（同 counter 的 reset）：先回执，
                            // 2s 后由延迟任务停产 + 收尾
                            let stop_tx = e.stop_tx.clone();
                            let mgr_tx = self_tx.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(VM_STOP_DELAY).await;
                                let _ = stop_tx.send(true);
                                let _ = mgr_tx.send(VmCmd::FinishStop(id)).await;
                            });
                            evidence.vm_event("STOP", id);
                            vm_snapshot(&vms, &evidence);
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
                            vm_snapshot(&vms, &evidence);
                        }
                    }
                }

                VmCmd::IsBusy(id, reply) => {
                    let busy = match vms.get(&id) {
                        Some(e) => {
                            let (pr, prx) = oneshot::channel();
                            if e.cmd_tx.send(VmProducerCmd::IsBusy(pr)).await.is_ok() {
                                prx.await.ok()
                            } else {
                                None
                            }
                        }
                        None => None,
                    };
                    let _ = reply.send(busy);
                }

                VmCmd::Subscribe(id, reply) => match vms.get(&id) {
                    Some(e) => {
                        let (pr, prx) = oneshot::channel();
                        let res = if e.cmd_tx.send(VmProducerCmd::Subscribe(pr)).await.is_ok() {
                            prx.await.unwrap_or(Err(SubErr::Busy))
                        } else {
                            Err(SubErr::Busy)
                        };
                        let _ = reply.send(res);
                    }
                    None => {
                        let _ = reply.send(Err(SubErr::NotFound));
                    }
                },
            }
        }
    });

    VmManager { tx }
}

fn vm_snapshot(vms: &HashMap<u64, VmEntry>, evidence: &Evidence) {
    let mut list: Vec<(u64, VmState)> = vms.iter().map(|(id, e)| (*id, e.state)).collect();
    list.sort_by_key(|(id, _)| *id);
    evidence.vm_snapshot(&list);
}

/// 每台 VM 一个生产者：与全局 hello task 同一套不变量（有界通道、try_send、
/// 满则丢弃计数）。收到 stop 信号后「停产不停服」：不再产出，但继续服务
/// 订阅——console 页签保持连接，只是安静了。
fn spawn_vm_producer(
    id: u64,
    mut cmd_rx: mpsc::Receiver<VmProducerCmd>,
    mut stop_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HELLO_INTERVAL);
        let mut n: u64 = 0;
        let mut out: Option<mpsc::Sender<String>> = None;
        let mut dropped = Arc::new(AtomicU64::new(0));

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => match cmd {
                    VmProducerCmd::IsBusy(reply) => {
                        let busy = out.as_ref().is_some_and(|tx| !tx.is_closed());
                        let _ = reply.send(busy);
                    }
                    VmProducerCmd::Subscribe(reply) => {
                        // 每 VM 独占：上一个 receiver 还活着就 Busy；
                        // 新订阅 = 新通道从头开始，不做历史回放
                        if out.as_ref().is_some_and(|tx| !tx.is_closed()) {
                            let _ = reply.send(Err(SubErr::Busy));
                        } else {
                            let (line_tx, line_rx) = mpsc::channel(HELLO_CHANNEL_CAPACITY);
                            dropped = Arc::new(AtomicU64::new(0));
                            out = Some(line_tx);
                            let _ = reply.send(Ok(HelloSubscription {
                                rx: line_rx,
                                dropped: Arc::clone(&dropped),
                            }));
                        }
                    }
                },

                Ok(_) = stop_rx.changed() => {
                    // VM 已停：ticker 分支被下面的 if 条件禁用，产出停止
                }

                _ = ticker.tick(), if *stop_rx.borrow() == false => {
                    n += 1;
                    let line = format!("vm{id} hello world #{n}");
                    if let Some(tx) = out.as_ref() {
                        match tx.try_send(line) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                dropped.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(TrySendError::Closed(_)) => {
                                dropped.fetch_add(1, Ordering::Relaxed);
                                out = None;
                            }
                        }
                    }
                }
            }
        }
    });
}
