//! 执行层句柄的持有者。
//!
//! HTTP 层（counter.rs / terminal.rs）永远只通过这里暴露的句柄与执行层通信，
//! 不直捣执行层状态（不变量 2）。本文件随 step 增长：
//!   step-0: 只有静态资产目录
//!   step-1: + counter task 的命令通道
//!   step-2: + hello task 的订阅句柄

use std::path::PathBuf;

/// 前端构建产物目录。Linux 有 page cache，直读 fs 即「按需缺页」（§7）。
fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist")
}

#[derive(Clone)]
pub struct AppState {
    pub dist: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        Self { dist: dist_dir() }
    }
}
