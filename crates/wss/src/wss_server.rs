use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::StreamExt;
use native_tls::TlsAcceptor;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_native_tls::TlsAcceptor as TokioTlsAcceptor;
use tokio_native_tls::native_tls::Identity;
use tokio_tungstenite::tungstenite::Result;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, accept_async};
use tracing::{error, trace, warn};

use crate::wss_socket::{WSSReader, WSSWriter, WssConnection};
use close_handle::CloseHandle;

// An accepted TCP peer can otherwise keep the listener task waiting forever by
// never completing its TLS or WebSocket upgrade handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PENDING_HANDSHAKES: usize = 256;

pub struct WSSServer {
    join: JoinHandle<()>,
}

#[async_trait]
pub trait WSSListenCallback {
    async fn cb(&mut self, rd: WSSReader, wr: WSSWriter);
}

impl WSSServer {
    pub async fn listen_wss(
        host: String,
        pfx: String,
        pfx_password: String,
        close: Arc<Mutex<CloseHandle>>,
        f: Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>,
    ) -> Result<WSSServer, Box<dyn std::error::Error>> {
        trace!("wss accept start:{}!", host);

        let mut file = File::open(pfx)?;
        let mut pkcs12 = vec![];
        file.read_to_end(&mut pkcs12)?;
        let pkcs12 = Identity::from_pkcs12(&pkcs12, &pfx_password)?;

        let listener = TcpListener::bind(host).await?;
        let acceptor = TlsAcceptor::builder(pkcs12).build()?;
        let acceptor = TokioTlsAcceptor::from(acceptor);
        let handshake_slots = Arc::new(Semaphore::new(MAX_PENDING_HANDSHAKES));

        let join = tokio::spawn(async move {
            loop {
                {
                    let c_ref = close.as_ref().lock().await;
                    if c_ref.is_closed() {
                        break;
                    }
                }

                let (stream, addr) = match listener.accept().await {
                    Err(e) => {
                        error!("TcpServer listener loop err:{}!", e);
                        continue;
                    }
                    Ok(stream) => stream,
                };
                trace!("wss accept client ip:{:?}", addr);

                // A stalled client may not hold up the listener loop. This
                // permit is retained only until TLS and WS handshakes finish.
                let permit = match handshake_slots.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        warn!(
                            "wss too many pending handshakes; dropping client ip:{:?}",
                            addr
                        );
                        continue;
                    }
                };
                let acceptor = acceptor.clone();
                let callback = f.clone();

                tokio::spawn(async move {
                    let tls_stream = match timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
                    {
                        Ok(Ok(stream)) => stream,
                        Ok(Err(e)) => {
                            error!("wss TLS handshake failed for {}: {}", addr, e);
                            return;
                        }
                        Err(_) => {
                            warn!("wss TLS handshake timed out for {}", addr);
                            return;
                        }
                    };

                    let tls_stream = MaybeTlsStream::NativeTls(tls_stream);
                    let websocket: WebSocketStream<MaybeTlsStream<TcpStream>> =
                        match timeout(HANDSHAKE_TIMEOUT, accept_async(tls_stream)).await {
                            Ok(Ok(stream)) => stream,
                            Ok(Err(e)) => {
                                error!("wss WebSocket handshake failed for {}: {}", addr, e);
                                return;
                            }
                            Err(_) => {
                                warn!("wss WebSocket handshake timed out for {}", addr);
                                return;
                            }
                        };

                    drop(permit);
                    let (write, read) = websocket.split();
                    let write = WssConnection::new(write);
                    let mut callback = callback.as_ref().lock().await;
                    callback
                        .cb(WSSReader::new(read, write.clone()), WSSWriter::new(write))
                        .await;
                });
            }
        });

        Ok(WSSServer { join })
    }

    pub async fn listen_ws(
        host: String,
        close: Arc<Mutex<CloseHandle>>,
        f: Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>,
    ) -> Result<WSSServer, Box<dyn std::error::Error>> {
        trace!("ws accept start:{}!", host);
        let listener = TcpListener::bind(host).await?;
        let handshake_slots = Arc::new(Semaphore::new(MAX_PENDING_HANDSHAKES));

        let join = tokio::spawn(async move {
            loop {
                {
                    let c_ref = close.as_ref().lock().await;
                    if c_ref.is_closed() {
                        break;
                    }
                }

                let (stream, addr) = match listener.accept().await {
                    Err(e) => {
                        error!("ws accept client err:{}", e);
                        continue;
                    }
                    Ok(stream) => stream,
                };
                trace!("ws accept client ip:{:?}", addr);

                let permit = match handshake_slots.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        warn!(
                            "ws too many pending handshakes; dropping client ip:{:?}",
                            addr
                        );
                        continue;
                    }
                };
                let callback = f.clone();

                tokio::spawn(async move {
                    let websocket: WebSocketStream<MaybeTlsStream<TcpStream>> = match timeout(
                        HANDSHAKE_TIMEOUT,
                        accept_async(MaybeTlsStream::Plain(stream)),
                    )
                    .await
                    {
                        Ok(Ok(stream)) => stream,
                        Ok(Err(e)) => {
                            error!("ws WebSocket handshake failed for {}: {}", addr, e);
                            return;
                        }
                        Err(_) => {
                            warn!("ws WebSocket handshake timed out for {}", addr);
                            return;
                        }
                    };

                    drop(permit);
                    let (write, read) = websocket.split();
                    let write = WssConnection::new(write);
                    let mut callback = callback.as_ref().lock().await;
                    callback
                        .cb(WSSReader::new(read, write.clone()), WSSWriter::new(write))
                        .await;
                });
            }
        });

        Ok(WSSServer { join })
    }

    pub async fn join(self) {
        let _ = self.join.await;
    }
}
