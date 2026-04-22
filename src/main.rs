//! sjtu 二进制入口：初始化 tracing + 错误收口 + ExitCode 返回。
//!
//! S2 起切到 tokio runtime —— `cas` 模块走 reqwest async；
//! 其余子命令（hello/login/logout/status）虽然是同步实现，
//! 也允许直接在 async 上下文里被调用（`tokio::task::block_in_place` 不需要，因为它们不阻塞 reactor）。

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    // RUST_LOG=debug 时能看到登录链路日志；默认 warn 以上。
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match sjtu_cli::cli::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
