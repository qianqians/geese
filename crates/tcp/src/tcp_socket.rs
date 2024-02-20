use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use async_trait::async_trait;
use tracing::{trace, error};

use net::{NetReaderCallback, NetWriter, NetReader, NetPack};

pub struct TcpReader {
    rd: ReadHalf<TcpStream>
}

impl TcpReader {
    pub fn new(_rd: ReadHalf<TcpStream>) -> TcpReader {
        TcpReader { 
            rd: _rd
        }
    }
}

impl NetReader for TcpReader {
    fn start(self, f: Arc<Mutex<Box<dyn NetReaderCallback + Send + 'static>>>) -> JoinHandle<()>
    {
        trace!("TcpReader NetReader start!");

        let mut _p = self;
        let f_clone = f.clone();
        tokio::spawn(async move {
            let mut buf = vec![0; 1024];
            let mut net_pack = NetPack::new();

            loop {
                match _p.rd.read(&mut buf).await {
                    Ok(0) => {
                        error!("network recv 0!");
                        return;
                    },
                    Ok(n) => {
                        net_pack.input(&buf[..n]);
                        while let Some(data) = net_pack.try_get_pack() {
                            let mut f_handle = f_clone.as_ref().lock().await;
                            f_handle.cb(data).await;
                            trace!("process data end!");
                        }
                    },
                    Err(err) => {
                        error!("network err:{}!", err);
                        return;
                    }
                }
            }
        })
    }
}

pub struct TcpWriter {
    wr: WriteHalf<TcpStream>, 
}

impl TcpWriter {
    pub fn new(_wr: WriteHalf<TcpStream>) -> TcpWriter {
        TcpWriter{
            wr: _wr
        }
    }
}

#[async_trait]
impl NetWriter for TcpWriter {
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

        match self.wr.write_all(&tmp_buf).await {
            Err(_) => {
                return false;
            },
            Ok(_) => {
                return true;
            }
        }
    }

    async fn close(&mut self) {
        let _ = self.wr.shutdown().await;
    }
}