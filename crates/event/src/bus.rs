//! 跨系统事件总线。
//!
//! 提供类型安全的消息发布/订阅机制，用于引擎各子系统间的解耦通信。
//! 单线程优先设计，通过 `flush_all()` 在每帧末尾分发事件。
//!
//! # 示例
//!
//! ```
//! use event::bus::{EventBus, EngineEvent};
//! use std::sync::{Arc, Mutex};
//!
//! let mut bus = EventBus::new();
//! let received = Arc::new(Mutex::new(Vec::new()));
//! let received_clone = received.clone();
//!
//! bus.subscribe::<EngineEvent>(Box::new(move |e| {
//!     received_clone.lock().unwrap().push(format!("{:?}", e));
//! }));
//!
//! bus.publish(EngineEvent::EntityCreated { entity_id: "player".into() });
//! bus.publish(EngineEvent::AssetModified { path: "/tex/hero.png".into() });
//!
//! bus.flush_all();
//!
//! assert_eq!(received.lock().unwrap().len(), 2);
//! ```

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// AnyChannel — 类型擦除的事件通道
// ---------------------------------------------------------------------------

/// 类型擦除的事件通道 trait。EventBus 通过此 trait 操作异构事件类型。
trait AnyChannel: Send + Sync {
    /// 分发队列中所有事件给订阅者。
    fn flush(&mut self);
    /// 用于向下转型为 EventChannel<E>。
    #[allow(dead_code)]
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// 具型事件通道：存储事件队列 + 订阅者回调列表。
struct EventChannel<E: Send + Sync + 'static> {
    queue: RwLock<Vec<E>>,
    subscribers: Vec<Box<dyn Fn(&E) + Send + Sync>>,
}

impl<E: Send + Sync + 'static> EventChannel<E> {
    fn new() -> Self {
        Self {
            queue: RwLock::new(Vec::new()),
            subscribers: Vec::new(),
        }
    }

    fn publish(&self, event: E) {
        self.queue.write().unwrap().push(event);
    }

    fn subscribe(&mut self, handler: Box<dyn Fn(&E) + Send + Sync>) {
        self.subscribers.push(handler);
    }
}

impl<E: Send + Sync + 'static> AnyChannel for EventChannel<E> {
    fn flush(&mut self) {
        let events = self.queue.write().unwrap().drain(..).collect::<Vec<_>>();
        for event in &events {
            for handler in &self.subscribers {
                handler(event);
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// 事件总线：按事件类型索引的发布/订阅系统。
///
/// 每帧调用 `flush_all()` 将队列中的事件分发给所有订阅者。
pub struct EventBus {
    channels: RwLock<HashMap<TypeId, Box<dyn AnyChannel>>>,
}

impl EventBus {
    /// 创建空的事件总线。
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    /// 发布事件。事件暂存于队列中，在 `flush_all()` 时批量分发。
    ///
    /// 使用 `&self`（内部可变性）以支持从多个订阅者发布。
    pub fn publish<E: Send + Sync + 'static>(&self, event: E) {
        let mut channels = self.channels.write().unwrap();
        let channel = channels
            .entry(TypeId::of::<E>())
            .or_insert_with(|| Box::new(EventChannel::<E>::new()));
        channel
            .as_any_mut()
            .downcast_mut::<EventChannel<E>>()
            .expect("TypeId mismatch in EventBus")
            .publish(event);
    }

    /// 订阅事件。处理器在每次 `flush_all()` 时被调用。
    ///
    /// 使用 `&mut self` 以确保订阅操作独占。
    pub fn subscribe<E: Send + Sync + 'static>(&mut self, handler: Box<dyn Fn(&E) + Send + Sync>) {
        let mut channels = self.channels.write().unwrap();
        let channel = channels
            .entry(TypeId::of::<E>())
            .or_insert_with(|| Box::new(EventChannel::<E>::new()));
        channel
            .as_any_mut()
            .downcast_mut::<EventChannel<E>>()
            .expect("TypeId mismatch in EventBus")
            .subscribe(handler);
    }

    /// 分发所有暂存的事件给对应的订阅者，然后清空队列。
    ///
    /// 应在每帧末尾调用一次。
    pub fn flush_all(&mut self) {
        let mut channels = self.channels.write().unwrap();
        for channel in channels.values_mut() {
            channel.flush();
        }
    }

    /// 清空所有事件队列和订阅者。
    pub fn clear(&mut self) {
        self.channels.write().unwrap().clear();
    }

    /// 已注册的事件类型数量。
    pub fn event_type_count(&self) -> usize {
        self.channels.read().unwrap().len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 标准引擎事件
// ---------------------------------------------------------------------------

/// 引擎级标准事件枚举。
///
/// 订阅者可通过 `bus.subscribe::<EngineEvent>(handler)` 监听。
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// 资源文件被修改（热重载触发）。
    AssetModified { path: String },
    /// 实体被创建。
    EntityCreated { entity_id: String },
    /// 实体被销毁。
    EntityDestroyed { entity_id: String },
    /// 引擎配置已更改。
    ConfigChanged,
    /// 帧开始。
    FrameStart,
    /// 帧结束。
    FrameEnd,
    /// 渲染开始。
    RenderStart,
    /// 渲染结束。
    RenderEnd,
    /// 窗口大小变更。
    WindowResize { width: u32, height: u32 },
}

// ---------------------------------------------------------------------------
// EventBusBridge — Scene 通过 trait object 持有的最小接口
// ---------------------------------------------------------------------------

/// 事件总线桥接 trait：Scene 通过 `Option<Box<dyn EventBusBridge>>` 持有。
///
/// trait 方法不含泛型参数，满足对象安全性。
pub trait EventBusBridge: Send + Sync {
    /// 分发所有暂存事件。
    fn flush(&mut self);
}

/// EventBus 对 EventBusBridge 的实现。
impl EventBusBridge for EventBus {
    fn flush(&mut self) {
        self.flush_all();
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn publish_and_flush() {
        let mut bus = EventBus::new();
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        bus.subscribe::<String>(Box::new(move |msg| {
            received_clone.lock().unwrap().push(msg.clone());
        }));

        bus.publish("hello".to_string());
        bus.publish("world".to_string());
        bus.flush_all();

        let r = received.lock().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], "hello");
        assert_eq!(r[1], "world");
    }

    #[test]
    fn multiple_subscribers() {
        let mut bus = EventBus::new();
        let count_a = Arc::new(Mutex::new(0usize));
        let count_b = Arc::new(Mutex::new(0usize));
        let a_clone = count_a.clone();
        let b_clone = count_b.clone();

        bus.subscribe::<i32>(Box::new(move |_| *a_clone.lock().unwrap() += 1));
        bus.subscribe::<i32>(Box::new(move |_| *b_clone.lock().unwrap() += 1));

        bus.publish(42);
        bus.flush_all();

        assert_eq!(*count_a.lock().unwrap(), 1);
        assert_eq!(*count_b.lock().unwrap(), 1);
    }

    #[test]
    fn different_event_types() {
        let mut bus = EventBus::new();
        let string_count = Arc::new(Mutex::new(0usize));
        let int_count = Arc::new(Mutex::new(0usize));
        let sc = string_count.clone();
        let ic = int_count.clone();

        bus.subscribe::<String>(Box::new(move |_| *sc.lock().unwrap() += 1));
        bus.subscribe::<i32>(Box::new(move |_| *ic.lock().unwrap() += 1));

        bus.publish("test".to_string());
        bus.publish(1);
        bus.publish(2);
        bus.flush_all();

        assert_eq!(*string_count.lock().unwrap(), 1);
        assert_eq!(*int_count.lock().unwrap(), 2);
    }

    #[test]
    fn engine_events() {
        let mut bus = EventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        bus.subscribe::<EngineEvent>(Box::new(move |e| {
            events_clone.lock().unwrap().push(format!("{:?}", e));
        }));

        bus.publish(EngineEvent::EntityCreated { entity_id: "e1".into() });
        bus.publish(EngineEvent::ConfigChanged);
        bus.flush_all();

        assert_eq!(events.lock().unwrap().len(), 2);
    }

    #[test]
    fn clear_removes_all() {
        let mut bus = EventBus::new();
        let count = Arc::new(Mutex::new(0usize));
        let count_clone = count.clone();

        bus.subscribe::<String>(Box::new(move |_| *count_clone.lock().unwrap() += 1));
        bus.publish("msg".to_string());
        bus.clear();
        bus.flush_all();

        assert_eq!(*count.lock().unwrap(), 0);
    }

    #[test]
    fn eventbusbridge_trait_object() {
        let mut bus = EventBus::new();
        let count = Arc::new(Mutex::new(0usize));
        let count_clone = count.clone();

        bus.subscribe::<String>(Box::new(move |_| *count_clone.lock().unwrap() += 1));
        bus.publish("test".to_string());

        let bridge: &mut dyn EventBusBridge = &mut bus;
        bridge.flush();

        assert_eq!(*count.lock().unwrap(), 1);
    }
}
