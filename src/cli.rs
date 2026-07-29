use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "antibot",
    version,
    about = "Security Protective Botnet Defensive — detecção e banimento de tráfego suspeito por volume"
)]
pub struct Cli {
    #[arg(long, default_value = "antibot.toml")]
    pub config: PathBuf,

    /// Sem subcomando nenhum: entra no modo interativo (/start /manutein /logout)
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Bootstrap automático: cria antibot.toml, .gitignore e git init — um comando só
    Setup,

    /// Cria/atualiza o schema do banco de dados
    InitDb,

    /// Monitora um servidor (local ou remoto) e aplica detecção em tempo real
    Watch {
        /// Nome do servidor, como definido em antibot.toml
        server: String,
    },

    /// Bane um IP manualmente e propaga na blacklist compartilhada
    Ban {
        ip: String,
        #[arg(long, default_value = "manual")]
        reason: String,
    },

    /// Remove um IP da blacklist (local + compartilhada)
    Unban { ip: String },

    /// Lista os IPs banidos atualmente
    ListBlacklist,

    /// Puxa entradas novas da blacklist compartilhada e aplica localmente
    Sync { server: String },
}
