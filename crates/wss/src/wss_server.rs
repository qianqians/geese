use std::sync::Arc;
use std::marker::Send;
use std::fs::File;
use std::io::Read;
use std::net::{TcpListener, TcpStream};

use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tracing::{trace, error};
use native_tls::{Identity, TlsAcceptor};
use tungstenite::{accept, WebSocket};
use tungstenite::stream::MaybeTlsStream;
use async_trait::async_trait;

use close_handle::CloseHandle;

use crate::wss_socket::{WSSReader, WSSWriter};

pub struct WSSServer{
    join: JoinHandle<()>
}

#[async_trait]
pub trait WSSListenCallback {
    async fn cb(&mut self, rd: WSSReader, wr: WSSWriter);
}

impl WSSServer {
    pub async fn listen_wss(
        host:String, 
        pfx:String, 
        f:Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>, 
        _close: Arc<Mutex<CloseHandle>>) -> Result<WSSServer, Box<dyn std::error::Error>>
    {
        trace!("wss accept start:{}!", host);

        let mut file = File::open(pfx).unwrap();
        let mut pkcs12 = vec![];
        file.read_to_end(&mut pkcs12).unwrap();
        let pkcs12 = Identity::from_pkcs12(&pkcs12, "hacktheplanet")?;
        
        let _listener = TcpListener::bind(host).unwrap();
        let acceptor = TlsAcceptor::builder(pkcs12).build()?;

        let _clone_close = _close.clone();
        let _f_clone = f.clone();

        let _join = tokio::spawn(async move {
            for stream in _listener.incoming() {
                let _s = match stream {
                    Err(e) => {
                        error!("wss accept client err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };
                trace!("wss accept client ip:{:?}", _s.peer_addr());

                let _acc_s = match acceptor.accept(_s) {
                    Err(e) => {
                        error!("wss accept acceptor.accept err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };
                let _tls_stream: MaybeTlsStream<TcpStream> = MaybeTlsStream::NativeTls(_acc_s);
                let mut _websocket: WebSocket<MaybeTlsStream<TcpStream>> = match accept(_tls_stream) {
                    Err(e) => {
                        error!("wss accept accept err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };

                let _client_arc = Arc::new(Mutex::new(_websocket));
                let mut f_handle = _f_clone.as_ref().lock().await;
                f_handle.cb(WSSReader::new(_client_arc.clone()), WSSWriter::new(_client_arc.clone())).await;

                let _c_ref = _clone_close.as_ref().lock().await;
                if _c_ref.is_closed() {
                    break;
                }              
            }
        });

        Ok(WSSServer {
            join: _join
        })
    }

    pub async fn listen_ws(
        host:String, 
        f:Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>, 
        _close: Arc<Mutex<CloseHandle>>) -> Result<WSSServer, Box<dyn std::error::Error>> 
    {
        trace!("ws accept start:{}!", host);
        let _listener = TcpListener::bind(host).unwrap();
        let _clone_close = _close.clone();
        let _f_clone = f.clone();

        let _join = tokio::spawn(async move {
            loop {   
                let stream = _listener.accept();
                let (_s, addr) = match stream {
                    Err(e) => {
                        error!("ws accept client err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };
                trace!("ws accept client ip:{:?}", addr);

                let mut _websocket: WebSocket<MaybeTlsStream<TcpStream>> = match accept(MaybeTlsStream::Plain(_s)) {
                    Err(e) => {
                        error!("ws accept MaybeTlsStream::Plain err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };

                let _client_arc = Arc::new(Mutex::new(_websocket));
                let mut f_handle = _f_clone.as_ref().lock().await;
                f_handle.cb(WSSReader::new(_client_arc.clone()), WSSWriter::new(_client_arc.clone())).await;

                let _c_ref = _clone_close.as_ref().lock().await;
                if _c_ref.is_closed() {
                    break;
                }              
            }
        });

        Ok(WSSServer {
            join: _join
        })
    }

    pub async fn join(self) {
        let _ = self.join.await;
    }

}