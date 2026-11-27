use crabrouter_model::{ContainerConfig, ContainerCreateRequest, ContainerPortBinding};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use super::*;

#[test]
fn decodes_multiplexed_and_plain_log_frames() {
    let mut framed = Vec::new();
    let msg1 = b"hello stdout\n";
    framed.extend_from_slice(&[1, 0, 0, 0]);
    framed.extend_from_slice(&(u32::try_from(msg1.len()).unwrap()).to_be_bytes());
    framed.extend_from_slice(msg1);

    let msg2 = b"oops stderr\n";
    framed.extend_from_slice(&[2, 0, 0, 0]);
    framed.extend_from_slice(&(u32::try_from(msg2.len()).unwrap()).to_be_bytes());
    framed.extend_from_slice(msg2);

    let lines = decode_docker_log_frames(&framed);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].stream, "stdout");
    assert_eq!(lines[0].line, "hello stdout");
    assert_eq!(lines[1].stream, "stderr");
    assert_eq!(lines[1].line, "oops stderr");

    let plain = decode_docker_log_frames(b"line one\r\nline two\n");
    assert_eq!(plain.len(), 2);
    assert_eq!(plain[0].stream, "stdout");
    assert_eq!(plain[0].line, "line one");
    assert_eq!(plain[1].line, "line two");
}

fn spawn_mock_uds_server(sock_path: &std::path::Path) -> tokio::task::JoinHandle<()> {
    let _ = std::fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path).expect("bind mock uds");
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let Ok(n) = stream.read(&mut buf).await else {
                    return;
                };
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("");

                let (status, content_type, body): (&str, &str, Vec<u8>) = match (method, path) {
                    ("GET", "/version") => (
                        "200 OK",
                        "application/json",
                        json!({"Version": "5.4.2", "ApiVersion": "1.44"})
                            .to_string()
                            .into_bytes(),
                    ),
                    ("GET", "/info") => (
                        "200 OK",
                        "application/json",
                        json!({
                            "Containers": 2,
                            "ContainersRunning": 1,
                            "ContainersStopped": 1,
                            "Images": 3
                        })
                        .to_string()
                        .into_bytes(),
                    ),
                    ("GET", p) if p.starts_with("/containers/json") => (
                        "200 OK",
                        "application/json",
                        json!([{
                            "Id": "abcdef1234567890",
                            "Names": ["/demo"],
                            "Image": "alpine:latest",
                            "State": "running",
                            "Status": "Up 5m",
                            "Created": 1_700_000_000_i64,
                            "Ports": [{"IP": "0.0.0.0", "PrivatePort": 80, "PublicPort": 8081, "Type": "tcp"}],
                            "NetworkSettings": {"Networks": {"podman": {}}}
                        }])
                        .to_string()
                        .into_bytes(),
                    ),
                    ("POST", p) if p.starts_with("/containers/create") => (
                        "201 Created",
                        "application/json",
                        json!({"Id": "new1234567890"}).to_string().into_bytes(),
                    ),
                    ("POST", "/containers/new1234567890/start" | "/containers/demo/stop") => {
                        ("204 No Content", "text/plain", Vec::new())
                    }
                    ("GET", p) if p.starts_with("/containers/demo/logs") => {
                        let mut frame = vec![1, 0, 0, 0, 0, 0, 0, 6];
                        frame.extend_from_slice(b"oklog\n");
                        ("200 OK", "application/vnd.docker.raw-stream", frame)
                    }
                    ("GET", "/images/json") => (
                        "200 OK",
                        "application/json",
                        json!([{
                            "Id": "sha256:112233",
                            "RepoTags": ["alpine:latest"],
                            "Size": 7_340_032_u64,
                            "Created": 1_700_000_000_i64
                        }])
                        .to_string()
                        .into_bytes(),
                    ),
                    _ => (
                        "404 Not Found",
                        "application/json",
                        json!({"message": "not found"}).to_string().into_bytes(),
                    ),
                };

                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
                let _ = stream.shutdown().await;
            });
        }
    })
}

#[tokio::test]
async fn socket_backend_communicates_over_unix_socket() {
    let dir = std::env::temp_dir().join(format!(
        "crabrouter-sock-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let sock_path = dir.join("podman.sock");
    let server = spawn_mock_uds_server(&sock_path);

    let cfg = ContainerConfig {
        enabled: true,
        socket_path: sock_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let backend = SocketContainerBackend;

    let info = backend.runtime_info(&cfg).await.expect("runtime_info");
    assert!(info.available);
    assert_eq!(info.version.as_deref(), Some("5.4.2"));
    assert_eq!(info.containers_running, 1);
    assert_eq!(info.images_count, 3);

    let list = backend.list_containers(&cfg, true).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "demo");
    assert_eq!(list[0].ports[0].host_port, 8081);

    let create_req = ContainerCreateRequest {
        name: "web1".to_string(),
        image: "alpine:latest".to_string(),
        cmd: Vec::new(),
        env: Vec::new(),
        ports: vec![ContainerPortBinding {
            host_ip: None,
            host_port: 8082,
            container_port: 80,
            protocol: "tcp".to_string(),
        }],
        mounts: Vec::new(),
        network: Some("podman".to_string()),
        restart_policy: None,
        privileged: false,
        start_immediately: true,
    };
    let created = backend
        .create_container(&cfg, &create_req)
        .await
        .expect("create");
    assert!(created.ok);
    assert_eq!(created.id.as_deref(), Some("new1234567890"));

    let stopped = backend
        .container_action(&cfg, "demo", ContainerAction::Stop)
        .await
        .expect("stop");
    assert!(stopped.ok);

    let logs = backend
        .container_logs(&cfg, "demo", 50)
        .await
        .expect("logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].line, "oklog");

    let imgs = backend.list_images(&cfg).await.expect("images");
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0].repo_tags, vec!["alpine:latest"]);

    server.abort();
    let _ = std::fs::remove_dir_all(&dir);
}
