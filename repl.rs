//! Modo automático do antibot: roda `antibot` sem argumento nenhum e cai
//! aqui. Só existem três comandos pra digitar — o resto (registrar
//! servidores, subir o schema, monitorar todos em paralelo, sincronizar a
//! blacklist compartilhada) acontece sozinho.

use crate::config::Config;
use crate::{db, ensure_server_registered, print_blacklist, run_watch, sync_server};
use sqlx::PgPool;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::JoinHandle;
use tracing::{error, info};

const SYNC_INTERVAL: Duration = Duration::from_secs(60);

pub async fn run(pool: PgPool, cfg: Config) -> anyhow::Result<()> {
    print_help();

    let mut watch_handles: Vec<JoinHandle<()>> = Vec::new();
    let mut sync_handle: Option<JoinHandle<()>> = None;
    let mut started = false;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        print!("> ");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let Some(line) = lines.next_line().await? else {
            break; // stdin fechou (ex: rodando sem terminal) — encerra de boa
        };

        match line.trim() {
            "/start" => {
                if started {
                    println!("já tá rodando — usa /manutein pra mexer manualmente.");
                    continue;
                }
                match start_all(pool.clone(), cfg.clone()).await {
                    Ok((wh, sh)) => {
                        watch_handles = wh;
                        sync_handle = Some(sh);
                        started = true;
                        println!(
                            "no ar — monitorando {} servidor(es), sync automático a cada {}s.",
                            cfg.servers.len(),
                            SYNC_INTERVAL.as_secs()
                        );
                    }
                    Err(e) => {
                        error!(error = %e, "falha ao iniciar");
                        println!("erro ao iniciar: {e}");
                    }
                }
            }

            "/manutein" | "/manutencao" | "/maintenance" => {
                maintenance_menu(&pool, &cfg, &mut lines).await?;
            }

            "/logout" => {
                println!("encerrando...");
                for h in watch_handles.drain(..) {
                    h.abort();
                }
                if let Some(h) = sync_handle.take() {
                    h.abort();
                }
                break;
            }

            "" => continue,

            other => {
                println!("comando '{other}' não existe. Só tem: /start /manutein /logout");
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!("Security Protective Botnet Defensive — modo automático");
    println!("  /start     inicia o monitoramento de todos os servidores do antibot.toml");
    println!("  /manutein  menu manual (listar / banir / desbanir)");
    println!("  /logout    encerra tudo e sai");
}

/// Sobe o schema (idempotente), registra todos os servidores do config,
/// e dispara: uma task de watch por servidor + uma task de sync periódico
/// que propaga a blacklist compartilhada pra todos eles.
async fn start_all(
    pool: PgPool,
    cfg: Config,
) -> anyhow::Result<(Vec<JoinHandle<()>>, JoinHandle<()>)> {
    db::init_schema(&pool).await?;

    if cfg.servers.is_empty() {
        anyhow::bail!("antibot.toml não tem nenhum servidor em [[servers]]");
    }

    for server_cfg in &cfg.servers {
        ensure_server_registered(&pool, server_cfg).await?;
    }

    let mut watch_handles = Vec::new();
    for server_cfg in &cfg.servers {
        let pool = pool.clone();
        let cfg = cfg.clone();
        let name = server_cfg.name.clone();
        watch_handles.push(tokio::spawn(async move {
            if let Err(e) = run_watch(pool, cfg, name.clone()).await {
                error!(server = %name, error = %e, "watch encerrou com erro");
            }
        }));
    }

    let sync_handle = tokio::spawn(sync_loop(pool, cfg));

    Ok((watch_handles, sync_handle))
}

/// Roda pra sempre em background, aplicando localmente qualquer ban novo
/// que outro servidor tenha detectado — é isso que faz a blacklist ser
/// "compartilhada" de fato sem precisar rodar `sync` na mão.
async fn sync_loop(pool: PgPool, cfg: Config) {
    loop {
        tokio::time::sleep(SYNC_INTERVAL).await;
        for server_cfg in &cfg.servers {
            match sync_server(&pool, server_cfg).await {
                Ok(n) if n > 0 => info!(server = %server_cfg.name, applied = n, "sync automático aplicou bans novos"),
                Ok(_) => {}
                Err(e) => error!(server = %server_cfg.name, error = %e, "falha no sync automático"),
            }
        }
    }
}

async fn maintenance_menu(
    pool: &PgPool,
    cfg: &Config,
    lines: &mut tokio::io::Lines<BufReader<tokio::io::Stdin>>,
) -> anyhow::Result<()> {
    println!("-- manutenção -- (listar | banir <ip> | desbanir <ip> | voltar)");
    loop {
        print!("manutein> ");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let Some(line) = lines.next_line().await? else {
            return Ok(());
        };
        let line = line.trim();
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("listar") => print_blacklist(pool).await?,
            Some("banir") => {
                let Some(ip) = parts.next() else {
                    println!("uso: banir <ip>");
                    continue;
                };
                crate::db::add_to_blacklist(pool, ip, "manual (manutein)", cfg.detector.ban_ttl_hours, "manutein")
                    .await?;
                crate::ban::apply_ban(ip).await?;
                println!("{ip} banido.");
            }
            Some("desbanir") => {
                let Some(ip) = parts.next() else {
                    println!("uso: desbanir <ip>");
                    continue;
                };
                crate::db::remove_from_blacklist(pool, ip).await?;
                crate::ban::remove_ban(ip).await?;
                println!("{ip} desbanido.");
            }
            Some("voltar") => return Ok(()),
            None => continue, // linha vazia — só reexibe o prompt
            _ => println!("comandos: listar | banir <ip> | desbanir <ip> | voltar"),
        }
    }
}
