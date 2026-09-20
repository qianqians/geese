use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{error, trace};

use net::{NetPack, NetReader, NetReaderCallback, NetReaderCloseCallback, NetWriter, notify_close};

pub type WssSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Both halves share ownership so retaining a writer cannot retain a closed socket.
pub struct WssConnection {
    sink: Mutex<Option<WssSink>>,
    closed: watch::Sender<bool>,
}

impl WssConnection {
    pub fn new(sink: WssSink) -> Arc<Self> {
        let (closed, _) = watch::channel(false);
        Arc::new(Self {
            sink: Mutex::new(Some(sink)),
            closed,
        })
    }

    fn disconnect(&self) {
        self.closed.send_replace(true);
    }

    async fn release(&self) {
        self.disconnect();
        // Wake any pending send before waiting for the sink lock.
        self.sink.lock().await.take();
    }

    async fn send(&self, message: Message) -> bool {
        let mut closed = self.closed.subscribe();
        let mut slot = self.sink.lock().await;
        if *closed.borrow() {
            slot.take();
            return false;
        }
        let Some(sink) = slot.as_mut() else {
            return false;
        };
        let result = tokio::select! {
            biased;
            _ = closed.changed() => None,
            result = sink.send(message) => Some(result),
        };
        if matches!(result, Some(Ok(()))) {
            return true;
        }
        if let Some(Err(err)) = result {
            error!("WSSWriter send failed: {}", err);
        }
        self.disconnect();
        slot.take();
        false
    }
}

pub struct WSSReader {
    s: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    writer: Arc<WssConnection>,
}

impl WSSReader {
    pub fn new(
        s: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        writer: Arc<WssConnection>,
    ) -> Self {
        Self { s, writer }
    }
}

impl Drop for WSSReader {
    fn drop(&mut self) {
        self.writer.disconnect();
        // Usually uncontended; also handles readers dropped before start or aborted.
        if let Ok(mut sink) = self.writer.sink.try_lock() {
            sink.take();
        }
        // If a send owns the lock, the close signal cancels it and it drops the sink.
    }
}

impl NetReader for WSSReader {
    fn start(
        mut self,
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>,
        close: Option<Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>>>,
    ) -> JoinHandle<()> {
        trace!("WSSReader NetReader start!");
        tokio::spawn(async move {
            let mut closed = self.writer.closed.subscribe();
            {
                let read = async {
                    let mut net_pack = NetPack::new();
                    'read: loop {
                        let message = match self.s.next().await {
                            Some(Ok(message)) => message,
                            Some(Err(err)) => {
                                error!("WSSReader read error: {}", err);
                                break;
                            }
                            None => break,
                        };
                        match message {
                            Message::Close(_) => break,
                            Message::Ping(data) => {
                                if !self.writer.send(Message::Pong(data)).await {
                                    break;
                                }
                            }
                            Message::Binary(buf) => {
                                net_pack.input(&buf);
                                loop {
                                    match net_pack.try_get_pack() {
                                        Ok(Some(data)) => f.lock().await.cb(data).await,
                                        Ok(None) => break,
                                        Err(err) => {
                                            error!("network pack error: {:?}", err);
                                            break 'read;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                };
                if !*closed.borrow() {
                    tokio::select! {
                        biased;
                        _ = closed.changed() => {},
                        _ = read => {},
                    }
                }
            }
            self.writer.release().await;
            // Drop the read half before callbacks, which may block or retain the writer.
            drop(self);
            notify_close(&close).await;
        })
    }
}

pub struct WSSWriter {
    s: Arc<WssConnection>,
}

impl WSSWriter {
    pub fn new(s: Arc<WssConnection>) -> Self {
        Self { s }
    }
}

#[async_trait]
impl NetWriter for WSSWriter {
    async fn send(&mut self, buf: &[u8]) -> bool {
        let mut packet = Vec::with_capacity(4 + buf.len());
        packet.extend_from_slice(&(buf.len() as u32).to_le_bytes());
        packet.extend_from_slice(buf);
        self.s.send(Message::Binary(packet)).await
    }

    async fn close(&mut self) {
        self.s.disconnect();
        let sink = self.s.sink.lock().await.take();
        if let Some(mut sink) = sink {
            // A stalled peer must not keep network resources alive indefinitely.
            let _ = timeout(Duration::from_secs(1), sink.close()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async};

    struct Callbacks(Arc<AtomicUsize>);

    #[async_trait]
    impl NetReaderCallback for Callbacks {
        async fn cb(&mut self, _: Vec<u8>) {}
    }

    #[async_trait]
    impl NetReaderCloseCallback for Callbacks {
        async fn on_close(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn pair() -> (
        WSSReader,
        WSSWriter,
        WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client =
            tokio::spawn(async move { connect_async(format!("ws://{address}")).await.unwrap().0 });
        let (stream, _) = listener.accept().await.unwrap();
        let socket = accept_async(MaybeTlsStream::Plain(stream)).await.unwrap();
        let (sink, stream) = socket.split();
        let connection = WssConnection::new(sink);
        (
            WSSReader::new(stream, connection.clone()),
            WSSWriter::new(connection),
            client.await.unwrap(),
        )
    }

    fn start(reader: WSSReader, count: Arc<AtomicUsize>) -> JoinHandle<()> {
        reader.start(
            Arc::new(Mutex::new(Box::new(Callbacks(count.clone())))),
            Some(Arc::new(Mutex::new(Box::new(Callbacks(count))))),
        )
    }

    async fn assert_released(writer: &mut WSSWriter, task: JoinHandle<()>, count: &AtomicUsize) {
        timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert!(writer.s.sink.lock().await.is_none());
        assert!(!writer.send(b"after close").await);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        writer.close().await;
        writer.close().await;
    }

    #[tokio::test]
    async fn peer_close_releases_retained_writer() {
        let (reader, mut writer, mut peer) = pair().await;
        let count = Arc::new(AtomicUsize::new(0));
        let task = start(reader, count.clone());
        peer.close(None).await.unwrap();
        assert_released(&mut writer, task, &count).await;
    }

    #[tokio::test]
    async fn abrupt_disconnect_releases_retained_writer() {
        let (reader, mut writer, peer) = pair().await;
        let count = Arc::new(AtomicUsize::new(0));
        let task = start(reader, count.clone());
        drop(peer);
        assert_released(&mut writer, task, &count).await;
    }

    #[tokio::test]
    async fn invalid_packet_closes_connection_without_close_callback() {
        let (reader, mut writer, mut peer) = pair().await;
        let task = reader.start(
            Arc::new(Mutex::new(Box::new(Callbacks(Arc::new(AtomicUsize::new(
                0,
            )))))),
            None,
        );
        peer.send(Message::Binary(u32::MAX.to_le_bytes().to_vec()))
            .await
            .unwrap();
        timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert!(writer.s.sink.lock().await.is_none());
        assert!(!writer.send(b"after error").await);
        let disconnected = timeout(Duration::from_secs(3), peer.next()).await.unwrap();
        assert!(matches!(
            disconnected,
            None | Some(Err(_)) | Some(Ok(Message::Close(_)))
        ));
    }

    #[tokio::test]
    async fn local_close_stops_idle_reader() {
        let (reader, mut writer, _peer) = pair().await;
        let count = Arc::new(AtomicUsize::new(0));
        let task = start(reader, count.clone());
        tokio::task::yield_now().await;
        writer.close().await;
        assert_released(&mut writer, task, &count).await;
    }

    #[tokio::test]
    async fn aborted_reader_releases_retained_writer() {
        let (reader, mut writer, _peer) = pair().await;
        let task = start(reader, Arc::new(AtomicUsize::new(0)));
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(writer.s.sink.lock().await.is_none());
        assert!(!writer.send(b"after abort").await);
    }
}
