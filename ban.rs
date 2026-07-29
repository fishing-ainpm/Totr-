use anyhow::Context;
use tokio::process::Command;

/// Detecta se `nft` está disponível; senão cai pra `iptables`.
async fn has_nft() -> bool {
    Command::new("which")
        .arg("nft")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Aplica o bloqueio do IP no firewall local. Requer privilégio (root/sudo)
/// pra rodar de fato — em dev/teste sem permissão isso vai falhar, o que é
/// esperado.
pub async fn apply_ban(ip: &str) -> anyhow::Result<()> {
    if has_nft().await {
        // usa uma table/chain dedicada pra não brigar com regras existentes
        let _ = Command::new("nft")
            .args(["add", "table", "inet", "antibot"])
            .status()
            .await;
        let _ = Command::new("nft")
            .args([
                "add", "chain", "inet", "antibot", "input",
                "{", "type", "filter", "hook", "input", "priority", "0", ";", "}",
            ])
            .status()
            .await;
        let status = Command::new("nft")
            .args([
                "add", "rule", "inet", "antibot", "input",
                "ip", "saddr", ip, "drop",
            ])
            .status()
            .await
            .context("falha ao rodar nft")?;
        anyhow::ensure!(status.success(), "nft retornou erro ao banir {}", ip);
    } else {
        let status = Command::new("iptables")
            .args(["-A", "INPUT", "-s", ip, "-j", "DROP"])
            .status()
            .await
            .context("falha ao rodar iptables")?;
        anyhow::ensure!(status.success(), "iptables retornou erro ao banir {}", ip);
    }
    Ok(())
}

pub async fn remove_ban(ip: &str) -> anyhow::Result<()> {
    if has_nft().await {
        // nft não tem "delete rule by match" direto sem handle; requer
        // buscar o handle da regra primeiro.
        let out = Command::new("nft")
            .args(["-a", "list", "chain", "inet", "antibot", "input"])
            .output()
            .await?;
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains(ip) {
                if let Some(handle) = line.rsplit("handle").nth(0) {
                    let handle = handle.trim();
                    let _ = Command::new("nft")
                        .args(["delete", "rule", "inet", "antibot", "input", "handle", handle])
                        .status()
                        .await;
                }
            }
        }
    } else {
        let _ = Command::new("iptables")
            .args(["-D", "INPUT", "-s", ip, "-j", "DROP"])
            .status()
            .await;
    }
    Ok(())
}
