use std::sync::Arc;
use std::marker::Send;

use tokio::io::{self};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use async_trait::async_trait;
use tracing::{trace, error};

use close_handle::CloseHandle;

use crate::tcp_socket::{TcpReader, TcpWriter};

pub struct TcpServer{
    join: JoinHandle<()>
}

#[async_trait]
pub trait TcpListenCallback {
    async fn cb(&mut self, rd: TcpReader, wr: TcpWriter);
}

impl TcpServer {
    pub async fn listen(
        host:String, 
        f:Arc<Mutex<Box<dyn TcpListenCallback + Send + 'static>>>, 
        _close: Arc<Mutex<CloseHandle>>) -> Result<TcpServer, Box<dyn std::error::Error>> 
    {
        trace!("tcp accept start:{}!", host);
        let _listener = TcpListener::bind(host.clone()).await?;
        trace!("TcpListener bind:{} complete", host);

        let _clone_close = _close.clone();
        let _f_clone = f.clone();
        let _join = tokio::spawn(async move {
            loop {
                let _s_listen = _listener.accept().await;
                let (socket, _) = match _s_listen {
                    Err(e) => {
                        error!("TcpServer listener loop err:{}!", e);
                        continue;
                    },
                    Ok(_s) => _s
                };

                trace!("tcp accept client ip:{:?}", socket.peer_addr());

                let _clone_c = _clone_close.clone();
                let (rd, wr) = io::split(socket);
                let mut f_handle = _f_clone.as_ref().lock().await;
                f_handle.cb(TcpReader::new(rd), TcpWriter::new(wr)).await;

                let _c_ref = _clone_close.as_ref().lock().await;
                if _c_ref.is_closed() {
                    break;
                }              
            }
        });

        Ok(TcpServer {
            join: _join
        })
    }

    pub async fn join(self) {
        let _ = self.join.await;
    }

}