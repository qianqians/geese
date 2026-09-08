use std::sync::Arc;

use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::net::TcpStream;
use futures_util::stream::{SplitSink, SplitStream};
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream};
use tokio_tungstenite::tungstenite::Message;
use async_trait::async_trait;
use tracing::{trace, info, error, warn};

use net::{NetReaderCallback, NetReaderCloseCallback, NetWriter, NetReader, NetPack, notify_close};

pub type WssSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

pub struct WSSReader {
    s: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    writer: Arc<Mutex<WssSink>>,
}


impl WSSReader {
    pub fn new(_s: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>, writer: Arc<Mutex<WssSink>>) -> WSSReader {
        WSSReader { 
            s: _s,
            writer,
        }
    }
}

impl NetReader for WSSReader {
    fn start(self, f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>, close: Option<Arc<Mutex<Box<dyn NetReaderCloseCallback + Send + 'static>>>>) -> JoinHandle<()>
    {
        trace!("WSSReader NetReader start!");

        let mut _p = self;
        let f_clone = f.clone();
        tokio::spawn(async move {
            let mut net_pack = NetPack::new();
            loop {
                let message = match _p.s.next().await {
                    None => {
                        error!("WSSReader stream closed!");
                        notify_close(&close).await;
                        return;
                    },
                    Some(Err(_)) => {
                        error!("WSSReader read msg error!");
                        notify_close(&close).await;
                        return;
                    },
                    Some(Ok(msg)) => msg,
                };

                match message {
                    Message::Close(_) => {
                        error!("network Close!");
                        notify_close(&close).await;
                        return;
                    },
                    Message::Ping(data) => {
                        info!("ping");
                        let writer = _p.writer.clone();
                        let mut w = writer.lock().await;
                        if let Err(e) = w.send(Message::Pong(data)).await {
                            warn!("Failed to send Pong: {}", e);
                        }
                    },
                    Message::Binary(buf) => {
                        net_pack.input(&buf[..]);
                        loop {
                            match net_pack.try_get_pack() {
                                Ok(Some(data)) => {
                                    let mut f_handle = f_clone.as_ref().lock().await;
                                    f_handle.cb(data).await;
                                },
                                Ok(None) => break,
                                Err(err) => {
                                    error!("network pack error:{:?}!", err);
                                    notify_close(&close).await;
                                    return;
                                }
                            }
                        }
                    },
                    _ => {}
                }
            }
        })
    }
}

pub struct WSSWriter {
    s: Arc<Mutex<WssSink>>,
}

impl WSSWriter {
    pub fn new(_s: Arc<Mutex<WssSink>>) -> WSSWriter {
        WSSWriter{
            s: _s,
        }
    }
}

#[async_trait]
impl NetWriter for WSSWriter {
    async fn send(&mut self, buf: &[u8]) -> bool {
        let len = buf.len();
        let len0 = (len & 0xff) as u8;
        let len1 = ((len >> 8) & 0xff) as u8;
        let len2 = ((len >> 16) & 0xff) as u8;
        let len3 = ((len >> 24) & 0xff) as u8;

        let mut tmp_buf = vec![0u8; 4];
        tmp_buf[0] = len0;
        tmp_buf[1] = len1;
        tmp_buf[2] = len2;
        tmp_buf[3] = len3;
        tmp_buf.extend_from_slice(buf);

        let msg = Message::Binary(tmp_buf);
        let mut sink = self.s.lock().await;
        match sink.send(msg).await {
            Ok(_) => {
                return true;
            },
            Err(err) => {
                error!("WSSWriter send faild, {}", err);
                return false;
            }
        }
    }

    async fn close(&mut self) {
        let mut sink = self.s.lock().await;
        let _ = sink.close().await;
    }
}
