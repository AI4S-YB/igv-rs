//! IGV live external TCP command listener (`--port 60151`).

use std::net::SocketAddr;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct SocketCommand {
    pub line: String,
    pub reply: oneshot::Sender<String>,
}

pub async fn spawn_socket_server(
    port: u16,
) -> Result<(SocketAddr, mpsc::UnboundedReceiver<SocketCommand>, JoinHandle<()>)> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = mpsc::unbounded_channel::<SocketCommand>();

    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                break;
            };
            let tx_conn = tx.clone();
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(rest) = trimmed.strip_prefix("echo ") {
                        let _ = writer.write_all(format!("{rest}\n").as_bytes()).await;
                        continue;
                    }
                    let (rtx, rrx) = oneshot::channel();
                    if tx_conn.send(SocketCommand { line: trimmed, reply: rtx }).is_err() {
                        break;
                    }
                    let resp = rrx.await.unwrap_or_else(|_| "ERROR\n".to_string());
                    let msg = if resp.ends_with('\n') {
                        resp
                    } else {
                        format!("{resp}\n")
                    };
                    if writer.write_all(msg.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    Ok((addr, rx, handle))
}
