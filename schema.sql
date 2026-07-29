-- Schema Postgres. Pra MySQL: troque INET por VARCHAR(45), BIGSERIAL por
-- BIGINT AUTO_INCREMENT, TIMESTAMPTZ por DATETIME.

CREATE TABLE IF NOT EXISTS servers (
    id          SERIAL PRIMARY KEY,
    name        TEXT UNIQUE NOT NULL,
    host        TEXT,               -- null se for 'local'
    log_path    TEXT NOT NULL,
    mode        TEXT NOT NULL CHECK (mode IN ('local', 'remote')),
    ssh_user    TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS traffic_log (
    id          BIGSERIAL PRIMARY KEY,
    server_id   INT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    ip          INET NOT NULL,
    user_agent  TEXT,
    path        TEXT,
    method      TEXT,
    status_code INT,
    seen_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_traffic_ip_time ON traffic_log (ip, seen_at);
CREATE INDEX IF NOT EXISTS idx_traffic_server_time ON traffic_log (server_id, seen_at);

-- Blacklist compartilhada: qualquer servidor que rode `antibot sync` puxa
-- as entradas daqui e aplica o ban local (iptables/nftables).
CREATE TABLE IF NOT EXISTS blacklist (
    ip          INET PRIMARY KEY,
    reason      TEXT NOT NULL,
    hits        INT NOT NULL DEFAULT 1,
    banned_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ,          -- null = permanente
    source_host TEXT                  -- qual servidor detectou primeiro
);

-- Controla, por servidor, quais entradas da blacklist já foram aplicadas
-- localmente (evita reaplicar ban repetido a cada sync).
CREATE TABLE IF NOT EXISTS applied_bans (
    server_id   INT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    ip          INET NOT NULL,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (server_id, ip)
);
