//! 纯 Rust 服务端引擎。
//!
//! Feature gate: `rust-server`。
//!
//! 使用 tokio 替代 asyncio，重新实现 `app.py` 的核心事件循环
//! （entity/player/physics/service 管理）。

use std::sync::Arc;
use tokio::sync::RwLock;

/// 纯 Rust 服务端应用状态。
pub struct NativeServer {
    /// 服务端配置
    config: Arc<RwLock<ServerConfig>>,
    /// 运行中标志
    running: bool,
}

/// 服务端配置。
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_players: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9000,
            max_players: 100,
        }
    }
}

impl NativeServer {
    /// 创建新的 Rust 服务端实例。
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            running: false,
        }
    }

    /// 启动服务端事件循环。
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.running = true;

        let config = self.config.read().await;
        eprintln!(
            "[rust-server] Starting native server on {}:{}",
            config.host, config.port
        );
        drop(config);

        // 主循环: 当前为 stub 实现。
        // 完整实现需要:
        // 1. 绑定 TCP/UDP socket
        // 2. 接受客户端连接 (gate)
        // 3. 实体/玩家/AOI 管理
        // 4. 物理步进
        // 5. 服务发现 (consul)
        while self.running {
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;

            // TODO: tick entity/physics/service loop
        }

        Ok(())
    }

    /// 优雅关闭。
    pub fn shutdown(&mut self) {
        eprintln!("[rust-server] Shutting down...");
        self.running = false;
    }
}

/// 服务端入口（可被 main.rs 或 bin 调用）。
#[cfg(feature = "rust-server")]
pub async fn run_native_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut server = NativeServer::new(config);
    server.run().await
}
