//! 物理引擎 RPC 客户端（RPC 数组协议版）。
//!
//! 复用 `crates/tcp` 和 `crates/net` 模块，通过 TCP + msgpack
//! 与 Python 物理服务器通信。
//!
//! 协议帧格式（与 `crates/net::NetPack` 兼容）：
//! ```text
//! [4 字节 LE 长度] + [msgpack 编码的 body]
//! ```
//!
//! 请求 body: `[method_str, args_array]` — msgpack 数组，与 RPC stubs 对齐
//! 响应 body: `[result1, result2, ...]` — 成功数组
//! 错误响应: `["__err__", err_str]`

use std::sync::Arc;

use net::{NetReader, NetReaderCallback, NetWriter};
use serde::{Deserialize, Serialize};
use tcp::tcp_connect::TcpConnect;
use tokio::sync::{Mutex, oneshot};

// ---------------------------------------------------------------------------
// 基础数据类型（与 physics_common.juggle struct 对齐）
// ---------------------------------------------------------------------------

/// 三维向量。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// 四元数。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

/// 碰撞体快照（调试渲染用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodySnapshot {
    pub id: String,
    pub position: Vec3,
    /// 四元数旋转（x, y, z, w）
    pub rotation: Quat,
}

/// 射线检测命中结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayHit {
    pub body_id: String,
    pub point: Vec3,
    pub normal: Vec3,
}

/// 碰撞接触事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub body1_id: String,
    pub body2_id: String,
    pub point: Vec3,
    pub normal: Vec3,
}

// ---------------------------------------------------------------------------
// RPC 响应解析
// ---------------------------------------------------------------------------

const ERR_TAG: &str = "__err__";

/// 从 msgpack 数组响应中解析结果。
/// - 成功: 返回 `Ok(array)` 供方法按索引取值
/// - 错误: `["__err__", err_str]` → 返回 `Err(err_str)`
fn parse_response(data: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    let arr: Vec<serde_json::Value> =
        rmp_serde::from_slice(data).map_err(|e| format!("decode response: {e}"))?;
    if arr.len() >= 2 {
        if let Some(tag) = arr[0].as_str() {
            if tag == ERR_TAG {
                let err_msg = arr[1].as_str().unwrap_or("unknown error").to_string();
                return Err(err_msg);
            }
        }
    }
    Ok(arr)
}

/// 从响应数组中安全取值，空数组时返回描述性错误而非 panic。
fn get_field<'a>(arr: &'a [serde_json::Value], idx: usize, name: &str) -> Result<&'a serde_json::Value, String> {
    arr.get(idx)
        .ok_or_else(|| format!("{name}: response array missing element [{idx}]"))
}

// ---------------------------------------------------------------------------
// 内部响应回调
//
// 死锁预防设计（编译期不变量）：
// ─────────────────────────────
// PhysicsReaderCallback **不持有任何共享数据缓冲区**（如 Arc<Mutex<Vec<u8>>>），
// 仅持有一个 oneshot::Sender。响应通过 channel 传递，而非写入共享缓冲后轮询。
//
// 这保证了：
//   1. cb() 只发送数据到 oneshot channel，不需要获取外部锁
//   2. call() 在释放 response_tx 锁之后才 await oneshot::Receiver
//   3. 不存在「持有锁 → 等待数据 → 数据写入需要同一把锁」的死锁路径
//
// ❌ 禁止添加 Arc<Mutex<Option<Vec<u8>>>> 类型的轮询缓冲字段！
//    如果需要缓冲，请使用 tokio channel（mpsc/oneshot），不要用 Mutex + 轮询。
// ---------------------------------------------------------------------------

struct PhysicsReaderCallback {
    response_tx: Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>,
}

// 编译期断言：确保 response_tx 不包含数据缓冲，防止回归到轮询模式。
// 如果有人在 PhysicsReaderCallback 中添加了 Vec<u8> 或类似缓冲字段，
// 请检查此断言并重新评估死锁风险。
const _: () = {
    assert!(
        std::mem::size_of::<PhysicsReaderCallback>()
            == std::mem::size_of::<Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>>(),
        "PhysicsReaderCallback must only contain the oneshot sender \
         (no polling buffers). See deadlock-prevention comment above."
    );
};

impl PhysicsReaderCallback {
    fn new() -> Self {
        Self {
            response_tx: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl NetReaderCallback for PhysicsReaderCallback {
    async fn cb(&mut self, data: Vec<u8>) {
        let mut guard = self.response_tx.lock().await;
        if let Some(tx) = guard.take() {
            let _ = tx.send(data);
        }
    }
}

// ---------------------------------------------------------------------------
// PhysicsClient
// ---------------------------------------------------------------------------

/// 物理引擎 TCP 客户端。
///
/// 封装 `tcp::TcpConnect` + `net::NetWriter/NetReader`，与 Python
/// `physics_editor_server.py` 通信（RPC 数组协议）。
pub struct PhysicsClient {
    writer: Mutex<Box<dyn NetWriter + Send>>,
    response_tx: Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>,
    reader_join: tokio::task::JoinHandle<()>,
}

impl PhysicsClient {
    /// 连接到物理服务器。
    pub async fn connect(host: &str) -> Result<Self, String> {
        let (reader, writer) = TcpConnect::connect(host.to_string())
            .await
            .map_err(|e| format!("TcpConnect failed: {e}"))?;

        let callback = PhysicsReaderCallback::new();
        let response_tx = callback.response_tx.clone();
        let cb: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>> =
            Arc::new(Mutex::new(Box::new(callback)));

        let reader_join = reader.start(cb);

        Ok(Self {
            writer: Mutex::new(Box::new(writer)),
            response_tx,
            reader_join,
        })
    }

    /// 发送 RPC 请求（数组格式）并等待响应。
    ///
    /// 请求 body: `[method_str, args_array]`
    async fn call(&self, method: &str, argvs: Vec<serde_json::Value>) -> Result<Vec<u8>, String> {
        // 编码为 msgpack 数组: [method, args]
        let request: (String, Vec<serde_json::Value>) =
            (method.to_string(), argvs);
        let body =
            rmp_serde::to_vec(&request).map_err(|e| format!("encode request: {e}"))?;

        // 持锁期间注册 oneshot channel 并发送请求，保证并发安全。
        let rx = {
            let mut tx_guard = self.response_tx.lock().await;
            let (tx, rx) = oneshot::channel();
            *tx_guard = Some(tx);

            {
                let mut writer = self.writer.lock().await;
                writer.send(&body).await;
            }
            rx
        };
        // 锁已释放，等待响应（不持有任何锁）。
        let timeout = tokio::time::Duration::from_secs(30);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(_)) => Err("response channel closed".to_string()),
            Err(_) => Err("timeout after 30s".to_string()),
        }
    }

    // ---- 便捷方法 ----

    /// 初始化物理世界。
    pub async fn init_physics(&self, gravity: [f32; 3]) -> Result<i64, String> {
        let argvs = vec![
            serde_json::json!(gravity[0]),
            serde_json::json!(gravity[1]),
            serde_json::json!(gravity[2]),
        ];
        let resp = self.call("init_physics", argvs).await?;
        let arr = parse_response(&resp)?;
        get_field(&arr, 0, "init_physics")?
            .as_i64()
            .ok_or_else(|| "init_physics: missing scene_id".to_string())
    }

    /// 从 `.scene.json` 加载碰撞体。
    pub async fn load_scene(&self, manifest_path: &str) -> Result<i64, String> {
        let argvs = vec![serde_json::json!(manifest_path)];
        let resp = self.call("load_scene", argvs).await?;
        let arr = parse_response(&resp)?;
        get_field(&arr, 0, "load_scene")?
            .as_i64()
            .ok_or_else(|| "load_scene: missing body_count".to_string())
    }

    /// 步进物理模拟。
    pub async fn step(&self, dt: f64) -> Result<(), String> {
        let argvs = vec![serde_json::json!(dt)];
        let resp = self.call("step_physics", argvs).await?;
        let _arr = parse_response(&resp)?;
        Ok(())
    }

    /// 获取所有碰撞体快照。
    pub async fn get_bodies(&self) -> Result<Vec<BodySnapshot>, String> {
        let resp = self.call("get_bodies", vec![]).await?;
        let arr = parse_response(&resp)?;
        let bodies: Vec<BodySnapshot> = serde_json::from_value(
            get_field(&arr, 0, "get_bodies")?.clone(),
        )
        .map_err(|e| format!("decode bodies: {e}"))?;
        Ok(bodies)
    }

    /// 获取碰撞事件。
    pub async fn get_contacts(&self) -> Result<Vec<ContactInfo>, String> {
        let resp = self.call("get_contacts", vec![]).await?;
        let arr = parse_response(&resp)?;
        let contacts: Vec<ContactInfo> = serde_json::from_value(
            get_field(&arr, 0, "get_contacts")?.clone(),
        )
        .map_err(|e| format!("decode contacts: {e}"))?;
        Ok(contacts)
    }

    /// 射线检测。
    pub async fn cast_ray(
        &self,
        origin: (f64, f64, f64),
        direction: (f64, f64, f64),
        max_toi: f64,
    ) -> Result<Option<RayHit>, String> {
        let argvs = vec![
            serde_json::json!(origin.0),
            serde_json::json!(origin.1),
            serde_json::json!(origin.2),
            serde_json::json!(direction.0),
            serde_json::json!(direction.1),
            serde_json::json!(direction.2),
            serde_json::json!(max_toi),
        ];
        let resp = self.call("cast_ray", argvs).await?;
        let arr = parse_response(&resp)?;
        // rsp: [ray_hit_dict|null, has_hit:bool]
        let has_hit = arr.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
        let hit_val = get_field(&arr, 0, "cast_ray")?;
        if !has_hit || hit_val.is_null() {
            return Ok(None);
        }
        let hit: RayHit = serde_json::from_value(hit_val.clone())
            .map_err(|e| format!("decode ray_hit: {e}"))?;
        Ok(Some(hit))
    }

    /// 重置物理世界。
    pub async fn reset(&self) -> Result<(), String> {
        let resp = self.call("reset_physics", vec![]).await?;
        let _arr = parse_response(&resp)?;
        Ok(())
    }
}

impl Drop for PhysicsClient {
    fn drop(&mut self) {
        self.reader_join.abort();
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期不变量：PhysicsReaderCallback 仅含 oneshot sender，无轮询缓冲。
    /// 若有人误加了 Arc<Mutex<Vec<u8>>> 等字段，此测试将编译失败。
    #[test]
    fn callback_has_no_polling_buffer() {
        assert_eq!(
            std::mem::size_of::<PhysicsReaderCallback>(),
            std::mem::size_of::<Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>>(),
            "PhysicsReaderCallback must not contain polling buffers"
        );
    }

    /// 模拟完整的请求-响应流程：
    /// 1. call() 注册 oneshot sender 并释放锁
    /// 2. cb() 获取锁，通过 sender 发送数据
    /// 3. call() 在锁外 await receiver，无死锁
    #[tokio::test]
    async fn oneshot_roundtrip_no_deadlock() {
        let callback = PhysicsReaderCallback::new();
        let response_tx = callback.response_tx.clone();

        // 模拟 call() 中注册 sender 的逻辑
        let rx = {
            let mut tx_guard = response_tx.lock().await;
            let (tx, rx) = oneshot::channel();
            *tx_guard = Some(tx);
            rx
            // tx_guard 在此处 drop，锁已释放
        };

        // 模拟 cb() 回调（在另一个任务中，获取同一把锁）
        let cb_task = {
            let tx = callback.response_tx.clone();
            tokio::spawn(async move {
                // 模拟网络延迟
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                let mut guard = tx.lock().await;
                if let Some(sender) = guard.take() {
                    let _ = sender.send(b"response_data".to_vec());
                }
            })
        };

        // 模拟 call() 中在锁外 await 响应的逻辑
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx).await;
        assert!(result.is_ok(), "should not timeout");
        let data = result.unwrap().unwrap();
        assert_eq!(data, b"response_data");

        cb_task.await.unwrap();
    }

    /// 验证 cb() 在 sender 已被消费后再次调用不会 panic（幂等性）。
    #[tokio::test]
    async fn cb_without_pending_request_is_noop() {
        let mut callback = PhysicsReaderCallback::new();
        // 没有注册 sender 时，cb 应该静默忽略
        callback.cb(b"orphan_data".to_vec()).await;
    }

    /// 验证超时路径：sender 被 drop 时 receiver 收到 RecvError。
    #[tokio::test]
    async fn closed_channel_returns_error() {
        let (_tx, rx) = oneshot::channel::<Vec<u8>>();
        drop(_tx); // 模拟 sender 被 drop
        let result = tokio::time::timeout(tokio::time::Duration::from_millis(100), rx).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_err(), "closed channel should return RecvError");
    }
}
