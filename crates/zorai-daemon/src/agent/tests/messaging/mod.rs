use super::*;
use crate::session_manager::SessionManager;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

mod part1;
mod part2;
mod part3;
mod part4;
mod part5;
mod part6;
mod part7;
mod part8;
mod part9;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("daemon crate dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

async fn spawn_recording_openai_server(recorded_bodies: Arc<StdMutex<VecDeque<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording openai server");
    let addr = listener.local_addr().expect("recording server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request from test client");
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push_back(body);

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Gateway reply ok\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::with_capacity(65536);
    let mut temp = [0u8; 4096];
    let headers_end = loop {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            return Ok(String::new());
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, ':');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    while buffer.len().saturating_sub(headers_end) < content_length {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
    }

    let available = buffer.len().saturating_sub(headers_end).min(content_length);
    Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
}
