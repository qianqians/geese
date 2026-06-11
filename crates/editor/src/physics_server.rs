//! Python 物理服务器进程管理。
//!
//! [`PhysicsServerManager`] 负责：
//! - 自动启动 Python 子进程（`physics_editor_server.py`）
//! - 端口探测与就绪等待
//! - Drop 时自动清理子进程
//!
//! 使用外部提供的 tokio Runtime 进行异步通信。

use std::net::TcpStream;
use std::process::{Child, Command};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::physics_client::PhysicsClient;

// ---------------------------------------------------------------------------
// PhysicsServerManager
// ---------------------------------------------------------------------------

/// Python 物理服务器进程管理器。
///
/// 自动启动 / 停止 Python 子进程，并通过外部 `tokio::runtime::Runtime` 建立
/// `PhysicsClient` 连接。
pub struct PhysicsServerManager {
    process: Option<Child>,
    client: Option<Arc<PhysicsClient>>,
    port: u16,
}

impl PhysicsServerManager {
    pub fn new() -> Self {
        Self {
            process: None,
            client: None,
            port: 9000,
        }
    }

    /// 查找可用端口并启动 Python 服务器。
    ///
    /// `server_script`: `physics_editor_server.py` 的绝对或相对路径。
    /// `rt`: 已有的 tokio Runtime，用于异步连接。
    pub fn start(
        &mut self,
        python_path: &str,
        server_script: &str,
        rt: &tokio::runtime::Runtime,
    ) -> Result<(), String> {
        // 1. 探测可用端口（从 self.port 开始）
        self.port = Self::find_free_port(self.port)?;

        // 2. 启动 Python 子进程
        let child = Command::new(python_path)
            .arg(server_script)
            .arg("--port")
            .arg(self.port.to_string())
            .spawn()
            .map_err(|e| format!("failed to spawn python server: {e}"))?;

        self.process = Some(child);

        // 3. 等待服务器就绪
        Self::wait_for_server(self.port, Duration::from_secs(5))?;

        // 4. 连接（在外部 Runtime 上执行）
        let addr = format!("127.0.0.1:{}", self.port);
        let client = rt.block_on(PhysicsClient::connect(&addr))?;
        self.client = Some(Arc::new(client));

        Ok(())
    }

    /// 是否已连接。
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// 获取客户端引用（Arc 克隆，可发给 spawned task）。
    pub fn client(&self) -> Option<Arc<PhysicsClient>> {
        self.client.clone()
    }

    /// 获取端口。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 停止服务器并清理。
    pub fn stop(&mut self) {
        // 先 drop client（关闭 TCP 连接和 reader task）
        self.client = None;
        // kill 子进程
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }

    // ---- 内部辅助 ----

    fn find_free_port(start_port: u16) -> Result<u16, String> {
        for port in start_port..start_port + 100 {
            if TcpStream::connect(format!("127.0.0.1:{}", port)).is_err() {
                return Ok(port);
            }
        }
        Err("no free port found in range 9000-9099".to_string())
    }

    fn wait_for_server(port: u16, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(stream) = TcpStream::connect(format!("127.0.0.1:{}", port)) {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!(
            "server startup timeout after {}s",
            timeout.as_secs()
        ))
    }
}

impl Drop for PhysicsServerManager {
    fn drop(&mut self) {
        self.stop();
    }
}
