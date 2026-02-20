use std::path::Path;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::info;

/// Unix domain socket IPC 서버
pub async fn listen(socket_path: &Path) -> Result<()> {
    // 기존 소켓 파일 제거
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path)?;
    info!("IPC socket listening on {:?}", socket_path);

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let response = handle_command(line.trim());
                let _ = writer.write_all(response.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                line.clear();
            }
        });
    }
}

fn handle_command(cmd: &str) -> String {
    match cmd {
        "ping" => "pong".into(),
        "stop" => {
            // 데몬 종료 시그널
            std::process::exit(0);
        }
        _ => format!("unknown command: {cmd}"),
    }
}

/// IPC 클라이언트: 소켓에 명령 전송
pub async fn send_command(socket_path: &Path, command: &str) -> Result<String> {
    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    writer.write_all(command.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    let mut reader = BufReader::new(reader);
    let mut response = String::new();
    reader.read_line(&mut response).await?;

    Ok(response.trim().to_string())
}
