use std::sync::Arc;
use std::marker::Send;
use std::net::TcpStream;

use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tungstenite::{WebSocket, Message};
use tungstenite::stream::MaybeTlsStream;
use async_trait::async_trait;
use tracing::{trace, error};

use net::{NetReaderCallback, NetWriter, NetReader, NetPack};
use close_handle::CloseHandle;

pub struct WSSReader {
    s: Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>
}


impl WSSReader {
    pub fn new(_s: Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>) -> WSSReader {
        WSSReader { 
            s: _s
        }
    }
}

impl NetReader for WSSReader {
    fn start(self, 
        f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>, 
        c: Arc<Mutex<CloseHandle>>) -> JoinHandle<()>
    {
        trace!("WSSReader NetReader start!");

        let mut _p = self;
        let f_clone = f.clone();
        tokio::spawn(async move {
            let mut net_pack = NetPack::new();
            loop {
                let message: Message;
                {
                    trace!("WSSReader get Message begin!");
                    let mut _client_ref = _p.s.as_ref().lock().await;
                    message = match _client_ref.read() {
                        Err(e) => {
                            error!("_client_ref error:{}", e);
                            break;
                        },
                        Ok(msg) => msg,
                    };
                    trace!("WSSReader Message::end!");
                }
                
                match message {
                    Message::Close(_) => {
                        error!("network Close!");

                        let message = Message::Close(None);
                        let mut _client_ref = _p.s.as_ref().lock().await;
                        _client_ref.send(message).unwrap();
                        return;
                    },
                    Message::Ping(ping) => {
                        trace!("WSSReader Message::Ping");

                        let message = Message::Pong(ping);
                        let mut _client_ref = _p.s.as_ref().lock().await;
                        _client_ref.send(message).unwrap();
                    },
                    Message::Binary(buf) => {
                        net_pack.input(&buf[..]);
                        match net_pack.try_get_pack() {
                            None => continue,
                            Some(data) => {
                                let mut f_handle = f_clone.as_ref().lock().await;
                                f_handle.cb(data).await;
                            }
                        }
                    },
                    _ => {}
                }

                let _c_ref = c.as_ref().lock().await;
                if _c_ref.is_closed() {
                    trace!("service closed!");
                    break;
                }
            }
        })
    }
}

pub struct WSSWriter {
    s: Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>
}

impl WSSWriter {
    pub fn new(_s: Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>) -> WSSWriter {
        WSSWriter{
            s: _s
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

        trace!("WSSWriter lock begin!");
        let mut wr = self.s.as_ref().lock().await;
        trace!("WSSWriter lock success!");
        let msg = Message::Binary(tmp_buf);
        {
            match wr.send(msg) {
                Err(_) => {
                    error!("WSS send faild!");
                    return false;
                },
                Ok(_) => {
                    return true;
                }
            }
        }
    }

    async fn close(&mut self) {
        let mut s = self.s.as_ref().lock().await;
        let _ = s.close(None);
    }
}
