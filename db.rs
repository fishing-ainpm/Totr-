use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct RequestEvent {
    pub server_id: i32,
    pub ip: String,
    pub user_agent: Option<String>,
    pub path: Option<String>,
    pub method: Option<String>,
    pub status_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct BlacklistEntry {
    pub ip: String,
    pub reason: String,
    pub hits: i32,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;
    Ok(pool)
}

/// Roda o schema.sql (idempotente — usa IF NOT EXISTS / ON CONFLICT).
pub async fn init_schema(pool: &PgPool) -> anyhow::Result<()> {
    let schema = include_str!("../schema.sql");
    for stmt in schema.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sqlx::query(stmt).execute(pool).await?;
    }
    Ok(())
}

pub async fn upsert_server(
    pool: &PgPool,
    name: &str,
    host: Option<&str>,
    log_path: &str,
    mode: &str,
    ssh_user: Option<&str>,
) -> anyhow::Result<i32> {
    let row = sqlx::query(
        r#"
        INSERT INTO servers (name, host, log_path, mode, ssh_user)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (name) DO UPDATE
            SET host = EXCLUDED.host,
                log_path = EXCLUDED.log_path,
                mode = EXCLUDED.mode,
                ssh_user = EXCLUDED.ssh_user
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(host)
    .bind(log_path)
    .bind(mode)
    .bind(ssh_user)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i32, _>("id"))
}

pub async fn server_id_by_name(pool: &PgPool, name: &str) -> anyhow::Result<Option<i32>> {
    let row = sqlx::query("SELECT id FROM servers WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<i32, _>("id")))
}

pub async fn insert_event(pool: &PgPool, ev: &RequestEvent) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO traffic_log (server_id, ip, user_agent, path, method, status_code)
        VALUES ($1, $2::text::inet, $3, $4, $5, $6)
        "#,
    )
    .bind(ev.server_id)
    .bind(&ev.ip)
    .bind(&ev.user_agent)
    .bind(&ev.path)
    .bind(&ev.method)
    .bind(ev.status_code)
    .execute(pool)
    .await?;
    Ok(())
}

/// Conta quantas requisições um IP fez nos últimos `window_seconds` segundos.
pub async fn count_recent_requests(
    pool: &PgPool,
    ip: &str,
    window_seconds: i64,
) -> anyhow::Result<i64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) as cnt FROM traffic_log
        WHERE ip = $1::text::inet
          AND seen_at > now() - ($2 || ' seconds')::interval
        "#,
    )
    .bind(ip)
    .bind(window_seconds.to_string())
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("cnt"))
}

pub async fn add_to_blacklist(
    pool: &PgPool,
    ip: &str,
    reason: &str,
    ttl_hours: i64,
    source_host: &str,
) -> anyhow::Result<()> {
    let expires_expr = if ttl_hours > 0 {
        format!("now() + interval '{} hours'", ttl_hours)
    } else {
        "NULL".to_string()
    };
    let sql = format!(
        r#"
        INSERT INTO blacklist (ip, reason, hits, expires_at, source_host)
        VALUES ($1::text::inet, $2, 1, {}, $3)
        ON CONFLICT (ip) DO UPDATE
            SET hits = blacklist.hits + 1,
                reason = EXCLUDED.reason
        "#,
        expires_expr
    );
    sqlx::query(&sql)
        .bind(ip)
        .bind(reason)
        .bind(source_host)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_from_blacklist(pool: &PgPool, ip: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM blacklist WHERE ip = $1::text::inet")
        .bind(ip)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_blacklist(pool: &PgPool) -> anyhow::Result<Vec<BlacklistEntry>> {
    let rows = sqlx::query(
        r#"SELECT ip::text as ip, reason, hits, expires_at FROM blacklist ORDER BY banned_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| BlacklistEntry {
            ip: r.get("ip"),
            reason: r.get("reason"),
            hits: r.get("hits"),
            expires_at: r.try_get("expires_at").ok(),
        })
        .collect())
}

/// Entradas da blacklist ainda não aplicadas localmente neste servidor.
pub async fn unapplied_blacklist(
    pool: &PgPool,
    server_id: i32,
) -> anyhow::Result<Vec<BlacklistEntry>> {
    let rows = sqlx::query(
        r#"
        SELECT b.ip::text as ip, b.reason, b.hits, b.expires_at
        FROM blacklist b
        LEFT JOIN applied_bans a ON a.ip = b.ip AND a.server_id = $1
        WHERE a.ip IS NULL
          AND (b.expires_at IS NULL OR b.expires_at > now())
        "#,
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| BlacklistEntry {
            ip: r.get("ip"),
            reason: r.get("reason"),
            hits: r.get("hits"),
            expires_at: r.try_get("expires_at").ok(),
        })
        .collect())
}

pub async fn mark_applied(pool: &PgPool, server_id: i32, ip: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO applied_bans (server_id, ip) VALUES ($1, $2::text::inet)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(server_id)
    .bind(ip)
    .execute(pool)
    .await?;
    Ok(())
}
