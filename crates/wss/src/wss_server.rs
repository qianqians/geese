use std::sync::Arc;
use std::fs::File;
use std::io::Read;

use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use futures_util::stream::StreamExt;
use tracing::{trace, error};
use native_tls::TlsAcceptor;
use tokio_native_tls::TlsAcceptor as TokioTlsAcceptor;
use tokio_native_tls::native_tls::Identity;
use tokio_tungstenite::{accept_async, WebSocketStream, MaybeTlsStream};
use tokio_tungstenite::tungstenite::Result;
use async_trait::async_trait;

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
        f:Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>) -> Result<WSSServer, Box<dyn std::error::Error>>
    {
        trace!("wss accept start:{}!", host);

        let mut file = File::open(pfx).unwrap();
        let mut pkcs12 = vec![];
        file.read_to_end(&mut pkcs12).unwrap();
        let pkcs12 = Identity::from_pkcs12(&pkcs12, "hacktheplanet")?;
        
        let _listener = TcpListener::bind(host).await.expect("Can't listen");
        let acceptor = TlsAcceptor::builder(pkcs12).build()?;
        let _tokio_acceptor = TokioTlsAcceptor::from(acceptor);

        let _f_clone = f.clone();

        let _join = tokio::spawn(async move {
            while let Ok((_s, _)) = _listener.accept().await {
                trace!("wss accept client ip:{:?}", _s.peer_addr());

                let _acc_s = match _tokio_acceptor.accept(_s).await {
                    Err(e) => {
                        error!("wss accept acceptor.accept err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };

                let _tls_stream: MaybeTlsStream<TcpStream> = MaybeTlsStream::NativeTls(_acc_s);
                let mut _websocket: WebSocketStream<MaybeTlsStream<TcpStream>> = match accept_async(_tls_stream).await {
                    Err(e) => {
                        error!("wss accept accept err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };

                let (write, read) = _websocket.split();

                let mut f_handle = _f_clone.as_ref().lock().await;
                f_handle.cb(WSSReader::new(read), WSSWriter::new(write)).await;
            }
        });

        Ok(WSSServer {
            join: _join
        })
    }

    pub async fn listen_ws(
        host:String, 
        f:Arc<Mutex<Box<dyn WSSListenCallback + Send + 'static>>>) -> Result<WSSServer, Box<dyn std::error::Error>> 
    {
        trace!("ws accept start:{}!", host);
        let _listener = TcpListener::bind(host).await.expect("Can't listen");
        let _f_clone = f.clone();

        let _join = tokio::spawn(async move {
            loop {   
                let stream = _listener.accept().await;
                let (_s, addr) = match stream {
                    Err(e) => {
                        error!("ws accept client err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };
                trace!("ws accept client ip:{:?}", addr);

                let mut _websocket: WebSocketStream<MaybeTlsStream<TcpStream>> = match accept_async(MaybeTlsStream::Plain(_s)).await {
                    Err(e) => {
                        error!("ws accept MaybeTlsStream::Plain err:{}", e);
                        continue;
                    },
                    Ok(s) => s
                };
                let (write, read) = _websocket.split();
                
                let mut f_handle = _f_clone.as_ref().lock().await;
                f_handle.cb(WSSReader::new(read), WSSWriter::new(write)).await;      
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