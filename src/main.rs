mod ban;
mod banner;
mod cli;
mod config;
mod db;
mod detector;
mod logparse;
mod repl;
mod setup;
mod source;

use clap::Parser;
use cli::{Cli, Commands};
use config::{Config, ServerConfig};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    banner::print_banner();
    let cli = Cli::parse();

    // Setup roda antes de tudo — é o comando que cria o antibot.toml,
    // então não pode depender dele já existir.
    if matches!(cli.command, Some(Commands::Setup)) {
        return setup::run();
    }

    let cfg = Config::load(&cli.config)?;
    let pool = db::connect(&cfg.database.url).await?;

    match cli.command {
        // Sem subcomando: modo interativo — só /start, /manutein, /logout.
        None => return repl::run(pool, cfg).await,

        Some(Commands::Setup) => unreachable!("tratado acima"),

        Some(Commands::InitDb) => {
            db::init_schema(&pool).await?;
            info!("schema aplicado com sucesso");
        }

        Some(Commands::Ban { ip, reason }) => {
            db::add_to_blacklist(&pool, &ip, &reason, cfg.detector.ban_ttl_hours, "manual")
                .await?;
            ban::apply_ban(&ip).await?;
            info!(%ip, "banido manualmente");
        }

        Some(Commands::Unban { ip }) => {
            db::remove_from_blacklist(&pool, &ip).await?;
            ban::remove_ban(&ip).await?;
            info!(%ip, "desbanido");
        }

        Some(Commands::ListBlacklist) => {
            print_blacklist(&pool).await?;
        }

        Some(Commands::Sync { server }) => {
            let server_cfg = cfg
                .find_server(&server)
                .ok_or_else(|| anyhow::anyhow!("servidor '{}' não encontrado no config", server))?
                .clone();
            sync_server(&pool, &server_cfg).await?;
        }

        Some(Commands::Watch { server }) => {
            run_watch(pool.clone(), cfg.clone(), server).await?;
        }
    }

    Ok(())
}

pub async fn print_blacklist(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let entries = db::list_blacklist(pool).await?;
    if entries.is_empty() {
        println!("blacklist vazia");
    }
    for e in entries {
        let exp = e
            .expires_at
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| "permanente".to_string());
        println!(
            "{:<20} hits={:<5} motivo={:<15} expira={}",
            e.ip, e.hits, e.reason, exp
        );
    }
    Ok(())
}

pub async fn ensure_server_registered(
    pool: &sqlx::PgPool,
    server_cfg: &ServerConfig,
) -> anyhow::Result<i32> {
    if let Some(id) = db::server_id_by_name(pool, &server_cfg.name).await? {
        return Ok(id);
    }
    db::upsert_server(
        pool,
        &server_cfg.name,
        server_cfg.host.as_deref(),
        &server_cfg.log_path,
        &server_cfg.mode,
        server_cfg.ssh_user.as_deref(),
    )
    .await
}

/// Aplica localmente as entradas da blacklist compartilhada ainda não
/// aplicadas nesse servidor. Usado tanto pelo comando manual `sync` quanto
/// pelo loop automático do modo REPL.
pub async fn sync_server(pool: &sqlx::PgPool, server_cfg: &ServerConfig) -> anyhow::Result<usize> {
    let server_id = ensure_server_registered(pool, server_cfg).await?;
    let pending = db::unapplied_blacklist(pool, server_id).await?;
    let mut applied = 0;
    for entry in &pending {
        if let Err(e) = ban::apply_ban(&entry.ip).await {
            error!(ip = %entry.ip, error = %e, "falha ao aplicar ban");
            continue;
        }
        db::mark_applied(pool, server_id, &entry.ip).await?;
        info!(ip = %entry.ip, server = %server_cfg.name, "ban aplicado via sync");
        applied += 1;
    }
    Ok(applied)
}

/// Monitora um servidor até a fonte de log fechar (normalmente só encerra
/// se o processo cair ou a task for abortada de fora). Recebe tudo por
/// valor pra poder ser usada com `tokio::spawn` em paralelo, uma por
/// servidor, no modo REPL.
pub async fn run_watch(pool: sqlx::PgPool, cfg: Config, server_name: String) -> anyhow::Result<()> {
    let server_cfg = cfg
        .find_server(&server_name)
        .ok_or_else(|| anyhow::anyhow!("servidor '{}' não encontrado no config", server_name))?
        .clone();
    let server_id = ensure_server_registered(&pool, &server_cfg).await?;

    let (tx, mut rx) = mpsc::channel::<String>(1024);

    // fonte de log roda numa task separada, emitindo linhas cruas
    let source_handle: tokio::task::JoinHandle<anyhow::Result<()>> = match server_cfg.mode.as_str() {
        "local" => {
            let path = server_cfg.log_path.clone();
            tokio::spawn(async move { source::tail_local(path, tx).await })
        }
        "remote" => {
            let host = server_cfg.host.clone().ok_or_else(|| {
                anyhow::anyhow!("servidor remoto '{}' sem 'host' no config", server_name)
            })?;
            let user = server_cfg.ssh_user.clone();
            let path = server_cfg.log_path.clone();
            tokio::spawn(async move {
                source::poll_remote(host, user, path, Duration::from_secs(5), tx).await
            })
        }
        other => anyhow::bail!("modo '{}' inválido (use 'local' ou 'remote')", other),
    };

    info!(server = %server_name, mode = %server_cfg.mode, "monitorando");

    while let Some(raw_line) = rx.recv().await {
        let Some(parsed) = logparse::parse_line(&raw_line) else {
            continue; // linha fora do formato esperado — ignora
        };

        let event = db::RequestEvent {
            server_id,
            ip: parsed.ip.clone(),
            user_agent: parsed.user_agent,
            path: parsed.path,
            method: parsed.method,
            status_code: parsed.status_code,
        };
        if let Err(e) = db::insert_event(&pool, &event).await {
            error!(error = %e, "falha ao gravar evento no banco");
            continue;
        }

        match detector::evaluate(&pool, &cfg.detector, &parsed.ip).await {
            Ok(detector::Verdict::Ban { count }) => {
                warn!(ip = %parsed.ip, count, "volume acima do threshold — banindo");
                let reason = format!("{} req em {}s", count, cfg.detector.window_seconds);
                if let Err(e) = db::add_to_blacklist(
                    &pool,
                    &parsed.ip,
                    &reason,
                    cfg.detector.ban_ttl_hours,
                    &server_name,
                )
                .await
                {
                    error!(error = %e, "falha ao gravar blacklist");
                }
                if let Err(e) = ban::apply_ban(&parsed.ip).await {
                    error!(ip = %parsed.ip, error = %e, "falha ao aplicar ban no firewall");
                } else {
                    let _ = db::mark_applied(&pool, server_id, &parsed.ip).await;
                }
            }
            Ok(detector::Verdict::Suspicious { count }) => {
                warn!(ip = %parsed.ip, count, "tráfego suspeito (abaixo do threshold de ban)");
            }
            Ok(detector::Verdict::Ok) => {}
            Err(e) => error!(error = %e, "falha ao avaliar tráfego"),
        }
    }

    source_handle.abort();
    Ok(())
}
