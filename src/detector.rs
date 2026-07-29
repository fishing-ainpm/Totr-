use crate::config::DetectorConfig;
use crate::db;
use sqlx::PgPool;

pub enum Verdict {
    Ok,
    Suspicious { count: i64 },
    Ban { count: i64 },
}

/// Consulta o volume de tráfego recente do IP e decide o veredito
/// com base nos thresholds configurados.
pub async fn evaluate(pool: &PgPool, cfg: &DetectorConfig, ip: &str) -> anyhow::Result<Verdict> {
    let count = db::count_recent_requests(pool, ip, cfg.window_seconds).await?;

    if count >= cfg.ban_threshold {
        Ok(Verdict::Ban { count })
    } else if count >= cfg.warn_threshold {
        Ok(Verdict::Suspicious { count })
    } else {
        Ok(Verdict::Ok)
    }
}
