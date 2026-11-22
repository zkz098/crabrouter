use std::path::Path;
use std::process::Stdio;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalClientFrame {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

pub async fn terminal_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(ws.on_upgrade(handle_terminal_ws))
}

async fn handle_terminal_ws(mut socket: WebSocket) {
    let mut child = match spawn_pty_shell() {
        Ok(c) => c,
        Err(err) => {
            let _ = socket
                .send(Message::Text(
                    format!(
                        "\r\n\x1b[31m[crabrouterd] failed to spawn PTY shell: {err}\x1b[0m\r\n"
                    )
                    .into(),
                ))
                .await;
            return;
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        return;
    };
    let Some(mut stdout) = child.stdout.take() else {
        return;
    };

    let _ = socket
        .send(Message::Text(
            "\x1b[38;5;208mCrabRouter Interactive PTY Terminal\x1b[0m (connected via WebSocket)\r\n"
                .into(),
        ))
        .await;

    let mut read_buf = [0_u8; 4096];
    loop {
        tokio::select! {
            read_res = stdout.read(&mut read_buf) => {
                match read_res {
                    Ok(0) | Err(_) => {
                        let _ = socket
                            .send(Message::Text("\r\n\x1b[33m[session closed]\x1b[0m\r\n".into()))
                            .await;
                        break;
                    }
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&read_buf[..n]).into_owned();
                        if socket.send(Message::Text(chunk.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(txt))) => {
                        if let Ok(frame) = serde_json::from_str::<TerminalClientFrame>(&txt) {
                            match frame {
                                TerminalClientFrame::Input { data } => {
                                    if stdin.write_all(data.as_bytes()).await.is_err() {
                                        break;
                                    }
                                    let _ = stdin.flush().await;
                                }
                                TerminalClientFrame::Resize { cols, rows } => {
                                    tracing::debug!(cols, rows, "xterm PTY resize received");
                                }
                            }
                        } else if stdin.write_all(txt.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(bin))) => {
                        if stdin.write_all(&bin).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = child.kill().await;
}

fn spawn_pty_shell() -> std::io::Result<Child> {
    let shell = if Path::new("/bin/bash").exists() {
        "/bin/bash -i"
    } else {
        "/bin/sh -i"
    };

    if Path::new("/usr/bin/script").exists() {
        return Command::new("/usr/bin/script")
            .args(["-q", "-f", "-c", shell, "/dev/null"])
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("COLUMNS", "120")
            .env("LINES", "32")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn();
    }

    let bin = if Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "/bin/sh"
    };
    Command::new(bin)
        .arg("-i")
        .env("TERM", "xterm-256color")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}
