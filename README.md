# Security Protective Botnet Defensive (antibot)

```
   [ SECURITY · PROTECTIVE · BOTNET · DEFENSIVE ]
   -----------------------------------------------
        volume anomaly detection · shared blacklist
```

CLI de detecção de tráfego suspeito por volume (rate anomaly) com blacklist
compartilhada entre servidores via banco de dados. O binário continua se
chamando `antibot` (nome do comando no terminal); "Security Protective
Botnet Defensive" é o nome de exibição, mostrado no banner ao rodar e em
`antibot --version`.

## Como funciona

1. `antibot watch <server>` lê o access log (nginx/apache formato "combined")
   — local via tail, ou remoto via SSH (`poll_remote`, sem precisar de sessão
   interativa) — e grava cada requisição em `traffic_log`.
2. A cada linha nova, checa quantas requisições aquele IP fez na janela
   configurada (`window_seconds`). Passou de `warn_threshold` → loga como
   suspeito. Passou de `ban_threshold` → insere na `blacklist` e aplica
   `DROP` via nftables (ou iptables, se nft não existir).
3. Outros servidores rodando `antibot sync <server>` puxam as entradas novas
   da blacklist compartilhada e aplicam o mesmo ban localmente — assim um IP
   banido num servidor já chega banido nos outros.

## Setup

Um comando só, automático — cria `antibot.toml` (a partir do template),
`.gitignore` e inicializa o repositório git, sem passo manual nenhum:

```bash
cargo build --release
./target/release/antibot setup
```

Depois só editar a connection string do banco dentro do `antibot.toml`
gerado, e criar as tabelas:

```bash
./target/release/antibot init-db
```

`setup` é idempotente — rodar de novo não sobrescreve o que já existe, só
completa o que falta (ex: se o `.gitignore` já existe mas não protege o
`antibot.toml`, ele completa em vez de substituir).

### MySQL em vez de Postgres

O código usa `sqlx` com a feature `postgres`. Pra usar MySQL:

1. No `Cargo.toml`, troque `"postgres"` por `"mysql"` na feature do sqlx.
2. Em `src/db.rs`, troque `PgPool`/`PgPoolOptions` por `MySqlPool`/`MySqlPoolOptions`.
3. Ajuste `schema.sql`: `INET` → `VARCHAR(45)`, `BIGSERIAL` → `BIGINT AUTO_INCREMENT`,
   `TIMESTAMPTZ` → `DATETIME`, e troque os `::inet`/`::text` casts (não existem em MySQL).
4. Placeholders do MySQL são `?` em vez de `$1, $2...` — ajusta as queries em `db.rs`.

O grosso da lógica (detector, ban, source, logparse) não muda nada.

## Modo automático (recomendado)

Depois do `setup` + `init-db`, rodar `antibot` sem nenhum argumento entra
num modo interativo com só três comandos — não precisa decorar nem digitar
subcomando nenhum toda vez:

```bash
antibot
```

```
> /start
no ar — monitorando 2 servidor(es), sync automático a cada 60s.

> /manutein
-- manutenção -- (listar | banir <ip> | desbanir <ip> | voltar)
manutein> listar
manutein> voltar

> /logout
encerrando...
```

- **`/start`** — sobe o schema se precisar, registra todos os servidores do
  `antibot.toml`, e dispara o monitoramento de todos eles em paralelo, mais
  um sync automático em background a cada 60s (propaga a blacklist
  compartilhada sozinho, sem precisar rodar `sync` na mão).
- **`/manutein`** — menu manual pra listar a blacklist ou banir/desbanir um
  IP na hora, sem sair do modo automático.
- **`/logout`** — encerra tudo (watch + sync) e sai.

Os subcomandos antigos (`watch`, `sync`, `ban` etc.) continuam existindo —
úteis pra scripts, cron ou systemd — mas no dia a dia o modo automático
resolve tudo.

## Uso (subcomandos manuais / scripting)

```bash
# monitorar um servidor (roda contínuo, Ctrl+C pra parar)
antibot watch meu-site-prod

# banir manualmente
antibot ban 203.0.113.9 --reason "scraper agressivo"

# tirar do ban
antibot unban 203.0.113.9

# ver blacklist atual
antibot list-blacklist

# sincronizar bans de outros servidores neste aqui
antibot sync meu-site-edge
```

## Rodar como serviço

Em produção, roda `antibot watch <server>` sob supervisão (systemd unit,
por exemplo), e `antibot sync <server>` num cron a cada 1-5min pra puxar bans
detectados em outros servidores.

## Permissões

`apply_ban`/`remove_ban` chamam `nft`/`iptables` diretamente — o processo
precisa rodar como root (ou com a capability certa) pra isso funcionar de
verdade. Sem privilégio, a chamada falha e loga o erro, mas o registro na
blacklist do banco continua acontecendo normalmente.

## Limitações conhecidas / próximos passos

- Detecção é só por volume bruto de requisições. Dá pra evoluir fácil
  adicionando sinais em `detector.rs`: consistência de user-agent, ausência
  de headers típicos de browser, entropia do padrão de acesso (bot tende a
  ter timing muito regular).
- `poll_remote` faz polling (não streaming real via SSH) — simples e
  reconectável, mas tem uma latência de até `interval` segundos pra detectar
  .
- Sem teste automatizado do binário em si aqui (ambiente sem acesso a rede
  pra baixar as crates e compilar) — revisa antes de rodar em produção.
