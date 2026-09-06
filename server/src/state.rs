//! 执行层句柄的持有者。
//!
//! HTTP 层（counter.rs / terminal.rs）永远只通过这里暴露的句柄与执行层通信，
//! 不直捣执行层状态（不变量 2）。本文件随 step 增长：
//!   step-0: 只有静态资产目录
//!   step-1: + counter task 的命令通道
//!   step-2: + hello task 的订阅句柄

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::error::TrySendError;

/// 前端构建产物目录。Linux 有 page cache，直读 fs 即「按需缺页」（§7）。
fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    pub counter: CounterHandle,
    pub hello: HelloHandle,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            dist: dist_dir(),
            counter: spawn_counter(),
            hello: spawn_hello(),
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

fn spawn_counter() -> CounterHandle {
    let (tx, mut rx) = mpsc::channel::<CounterCmd>(32);
    // reset 用的延迟提交通道：执行层自己给自己发命令，HTTP 层不参与
    let commit_tx = tx.clone();

    tokio::spawn(async move {
        let mut value: i64 = 0;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                CounterCmd::Get(reply) => {
                    let _ = reply.send(value);
                }
                CounterCmd::Add(delta, reply) => {
                    value += delta;
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
                CounterCmd::CommitReset => value = 0,
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
