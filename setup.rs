use std::path::Path;
use tracing::{info, warn};

const GITIGNORE_CONTENT: &str = "/target\nantibot.toml\n*.env\n*.pem\n*.key\nid_rsa*\n";
const GITIGNORE_MARKER: &str = "antibot.toml";

/// Bootstrap completo do projeto: garante antibot.toml, .gitignore e repo git
/// prontos, sem nenhum passo manual. Idempotente — rodar de novo não quebra nada.
pub fn run() -> anyhow::Result<()> {
    ensure_config()?;
    ensure_gitignore()?;
    ensure_git_repo()?;
    info!("setup completo — antibot.toml, .gitignore e git prontos");
    Ok(())
}

fn ensure_config() -> anyhow::Result<()> {
    let cfg_path = Path::new("antibot.toml");
    if cfg_path.exists() {
        info!("antibot.toml já existe — mantido como está");
        return Ok(());
    }
    let example_path = Path::new("antibot.example.toml");
    if !example_path.exists() {
        anyhow::bail!(
            "não achei antibot.example.toml — roda o setup na raiz do projeto"
        );
    }
    std::fs::copy(example_path, cfg_path)?;
    info!("antibot.toml criado a partir do template — edita a connection string do banco antes de usar");
    Ok(())
}

fn ensure_gitignore() -> anyhow::Result<()> {
    let path = Path::new(".gitignore");
    if !path.exists() {
        std::fs::write(path, GITIGNORE_CONTENT)?;
        info!(".gitignore criado");
        return Ok(());
    }

    let existing = std::fs::read_to_string(path)?;
    if existing.lines().any(|l| l.trim() == GITIGNORE_MARKER) {
        info!(".gitignore já protege antibot.toml");
        return Ok(());
    }

    // .gitignore existe mas não cobre o essencial — completa sem sobrescrever o que já tem
    let mut updated = existing;
    if !updated.ends_with('\n') && !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(GITIGNORE_CONTENT);
    std::fs::write(path, updated)?;
    warn!(".gitignore existia mas não cobria antibot.toml/segredos — completado automaticamente");
    Ok(())
}

fn ensure_git_repo() -> anyhow::Result<()> {
    if Path::new(".git").is_dir() {
        info!("repositório git já inicializado");
        return Ok(());
    }
    let status = std::process::Command::new("git").arg("init").status();
    match status {
        Ok(s) if s.success() => info!("git init rodado"),
        Ok(_) => warn!("git init retornou erro — inicializa manualmente se precisar"),
        Err(_) => warn!("git não encontrado no PATH — pulei o git init"),
    }
    Ok(())
}
