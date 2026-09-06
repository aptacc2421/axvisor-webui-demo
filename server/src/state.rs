//! 执行层句柄的持有者。
//!
//! HTTP 层（counter.rs / terminal.rs）永远只通过这里暴露的句柄与执行层通信，
//! 不直捣执行层状态（不变量 2）。本文件随 step 增长：
//!   step-0: 只有静态资产目录
//!   step-1: + counter task 的命令通道
//!   step-2: + hello task 的订阅句柄

use std::{path::PathBuf, time::Duration};
use tokio::sync::{mpsc, oneshot};

/// 前端构建产物目录。Linux 有 page cache，直读 fs 即「按需缺页」（§7）。
fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
    pub counter: CounterHandle,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            dist: dist_dir(),
            counter: spawn_counter(),
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
