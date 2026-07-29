use anyhow::Context;
use std::io::{Seek, SeekFrom};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
use tokio::sync::mpsc;

/// Segue um arquivo de log local, tipo `tail -f`, emitindo linha por linha.
pub async fn tail_local(path: String, tx: mpsc::Sender<String>) -> anyhow::Result<()> {
    let file = tokio::fs::File::open(&path)
        .await
        .with_context(|| format!("não consegui abrir {}", path))?;
    let mut reader = tokio::io::BufReader::new(file);
    // começa do fim do arquivo (só novas linhas a partir de agora)
    reader.seek(SeekFrom::End(0)).await?;

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // sem linha nova ainda — espera um pouco (log pode ter sido rotacionado)
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            if tx.send(trimmed.to_string()).await.is_err() {
                break; // receiver fechou
            }
        }
    }
    Ok(())
}

/// Faz polling num arquivo de log remoto via SSH (`tail -n`), comparando
/// contagem de linhas já vistas pra só emitir as novas. Não usa `tail -f`
/// interativo pra manter a conexão simples e reconectável.
pub async fn poll_remote(
    host: String,
    user: Option<String>,
    path: String,
    interval: Duration,
    tx: mpsc::Sender<String>,
) -> anyhow::Result<()> {
    use openssh::{KnownHosts, Session};

    let dest = match &user {
        Some(u) => format!("{}@{}", u, host),
        None => host.clone(),
    };

    let mut session = Session::connect(&dest, KnownHosts::Strict)
        .await
        .with_context(|| format!("não consegui conectar via SSH em {}", dest))?;

    let mut last_line_count: u64 = {
        let out = session
            .command("wc")
            .arg("-l")
            .arg(&path)
            .output()
            .await?;
        let s = String::from_utf8_lossy(&out.stdout);
        s.split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };

    loop {
        tokio::time::sleep(interval).await;

        // reconecta se a sessão caiu
        if session.check().await.is_err() {
            session = Session::connect(&dest, KnownHosts::Strict).await?;
        }

        let out = session.command("wc").arg("-l").arg(&path).output().await?;
        let current: u64 = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(last_line_count);

        if current > last_line_count {
            let new_lines = current - last_line_count;
            let out = session
                .command("tail")
                .arg("-n")
                .arg(new_lines.to_string())
                .arg(&path)
                .output()
                .await?;
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if !line.trim().is_empty() && tx.send(line.to_string()).await.is_err() {
                    return Ok(());
                }
            }
            last_line_count = current;
        }
    }
}
