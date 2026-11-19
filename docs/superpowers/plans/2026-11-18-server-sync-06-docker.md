# PMS Server — Implementation Plan 6: Docker Compose для развёртывания

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сервер поднимается одной командой `docker compose up -d` на чистом VPS: образ собирается из репозитория, данные и сертификаты живут в томах, обновление — смена тега и `docker compose up -d`.

**Architecture:** Multi-stage образ: сборка в `rust:1-slim` (нужен `cmake` для `aws-lc-rs`, `libsqlite3` не нужен — `rusqlite` собирается bundled), рантайм — `debian:bookworm-slim` с `ca-certificates` и `tzdata` и непривилегированным пользователем. **Сертификат выбирается здесь, при развёртывании** (пожелание пользователя, §18.2 спеки): либо `caddy` терминирует TLS и сам получает/продлевает Let's Encrypt, либо сервер выпускает самоподписанный сертификат на 10 лет, который продлевает cron. Compose описывает только то, что нужно одному VPS: ресурсы, healthcheck, ротацию логов, бэкап-скрипт.

**Tech Stack:** Docker Engine 27+, Docker Compose v2, BuildKit (multi-arch `linux/amd64` + `linux/arm64`), Caddy 2 (ACME, автопродление), `openssl` в деплой-скрипте (самоподписанный на 10 лет), cron для продления самоподписанного и для бэкапа, healthcheck через `curl /v1/healthz`.

**Spec:** `docs/superpowers/specs/2026-11-18-server-sync-design.md` (§8 хранилище, §11 TLS, §12 пределы)
**Предыдущие планы:** `...-01-server.md` … `...-05-android.md`
**Текущий этап:** рабочий прототип (`docs/reports/2026-11-19-prototype-status.md`)

## Global Constraints

- `tls.required = true` в образе по умолчанию: контейнер **не стартует** без сертификата — это проверяемое поведение, а не пожелание.
- Данные только в томе `/var/lib/pms`; контейнер запускается с `read_only` корневой ФС, `tmpfs` для `/tmp`.
- Процесс работает от `pms` (uid 10001), не от root; `no-new-privileges: true`.
- Квота по умолчанию 1 ГиБ на аккаунт; лимит памяти контейнера — `256M` (замер прототипа: 18 МиБ RSS в простое).
- Образ не содержит мастер-ключей и не умеет их принимать: сервер видит только шифротекст.
- Никаких секретов в compose-файле: пароли почты ACME и токен регистрации — через `.env`, который в `.gitignore`.
- ACME не встраивается в `pms-server` (§11): сертификаты выпускает Caddy (Let's Encrypt) или деплой-скрипт (самоподписанный на 10 лет с SAN).
- Два режима TLS, оба поддерживаются и оба с автопродлением: `acme` (Caddy) и `self-signed` (скрипт + cron). Выбор — переменная в `.env`; в обоих случаях клиент проверяет сервер по отпечатку.
- В режиме `acme` TLS терминирует Caddy, а `pms-server` слушает только внутреннюю сеть compose (`tls.required = false`, порт не публикуется). Это ослабление fail-closed, поэтому оно явно записано в README и в конфиге, и не является умолчанием.
- В режиме `self-signed` можно оставить TLS на самом `pms-server` (`tls.required = true`, сертификат в томе) — это вариант по умолчанию для домашней сети.

## Review Focus

1. Контейнер без смонтированного сертификата должен падать на старте, а не отдавать http.
2. Перезапуск контейнера не должен терять данные: том монтируется, `recount` на старте видит файлы.
3. Обновление образа не должно ломать формат: сервер читает данные, созданные предыдущим образом.
4. Healthcheck не должен проходить, когда процесс жив, но не отвечает.
5. Certbot-хук не должен оставлять сервер с просроченным сертификатом после неудачного продления.
6. Бэкап не должен останавливать сервер и не должен копировать полузаписанный WAL.

---

### Task 1: Образ

**Files:** Create `deploy/docker/Dockerfile`, `deploy/docker/.dockerignore`, `deploy/docker/build.sh`.

**Interfaces:** `docker build -f deploy/docker/Dockerfile -t pms-server:<tag> .` из корня репозитория; multi-stage: `builder` (rust + cmake + `cargo build --profile server -p pms-server --locked`) и `runtime` (debian-slim, `COPY --from=builder /app/target/server/pms-server /usr/local/bin/pms-server`).

- [ ] **Step 1:** Написать `Dockerfile` с кэшем зависимостей (сначала `Cargo.toml`/`Cargo.lock`, потом исходники), чтобы повторная сборка не качала крейты заново.
- [ ] **Step 2:** Проверить размер и запуск: `docker build … && docker run --rm pms-server:<tag> check-config --config /etc/pms/server.toml` без сертификата → код 1 с текстом «tls не готов».
- [ ] **Step 3:** Проверить multi-arch: `docker buildx build --platform linux/amd64,linux/arm64` доходит до конца (ARM-таргеты нужны ультрабюджетным VPS).
- [ ] **Step 4:** Commit.

### Task 2: Сертификат при развёртывании (два режима)

**Files:** Create `deploy/docker/cert-self-signed.sh`, `deploy/docker/Caddyfile`, `deploy/docker/renew-self-signed.sh`; Modify `deploy/docker/compose.yaml`, `.env.example`.

- [ ] **Step 1:** `cert-self-signed.sh`: `openssl req -x509 -newkey rsa:3072 -sha256 -days 3650` с обязательным `subjectAltName` из `PMS_DOMAIN`/`PMS_IP`, права 640 на ключ, владелец — `pms`.
- [ ] **Step 2:** `acme`-профиль: Caddyfile с `tls { ... }` и reverse_proxy на `pms-server:8443`; продление — встроенное у Caddy, cron не нужен.
- [ ] **Step 3:** `renew-self-signed.sh` + cron: перевыпуск за 30 дней до истечения, рестарт `pms-server` только при успешном перевыпуске (иначе сервер остаётся на старом рабочем сертификате).
- [ ] **Step 4:** Проверить оба режима: `docker compose --profile acme up -d` (или `staging`-ACME) и `PMS_TLS=self-signed docker compose up -d`; в обоих `curl` получает `healthz`, `openssl x509 -dates` показывает ожидаемый срок.
- [ ] **Step 5:** Commit.

### Task 3: Compose-файл

**Files:** Create `deploy/docker/compose.yaml`, `deploy/docker/server.toml`, `deploy/docker/.env.example`.

**Interfaces:**
- Сервис `pms-server`: образ, `ports: ["8443:8443"]`, тома `pms-data:/var/lib/pms` и `pms-certs:/etc/pms:ro`, `read_only: true`, `tmpfs: /tmp`, `user: "10001:10001"`, `mem_limit: 256m`, `restart: unless-stopped`, `healthcheck` (`curl -fsS --cacert /etc/pms/server.crt https://127.0.0.1:8443/v1/healthz`), логи `json-file` с ротацией 10 МБ × 3.
- Сервис `certbot` (профиль `acme`): образ `certbot/certbot`, том `pms-certs`, `entrypoint` с `certonly --standalone` + `trap` на продление, deploy-hook `docker compose restart pms-server`.

- [ ] **Step 1:** Описать compose и конфиг; в `server.toml` — `tls.required = true`, `mode = "files"`, пути в `/etc/pms`, `quota.default_bytes = 1073741824`.
- [ ] **Step 2:** Проверить с самоподписанным сертификатом в томе: `openssl req … -addext "subjectAltName=…"` (SAN обязателен), `docker compose up -d`, `curl --cacert` получает `{"protocol":1,"status":"ok"}`.
- [ ] **Step 3:** Проверить отказ: без сертификата в томе контейнер уходит в `Restarting` с понятным логом; `docker compose ps` это показывает.
- [ ] **Step 4:** Commit.

### Task 4: Данные, бэкап и обновление

**Files:** Create `deploy/docker/backup.sh`, `deploy/docker/compose.backup.yaml`, `deploy/docker/README.md`; Modify `deploy/README.md` (ссылка на compose-вариант).

**Interfaces:** `backup.sh` → `sqlite3 /var/lib/pms/index.sqlite ".backup …"` внутри контейнера + `tar` каталога `accounts/` в примонтированный каталог бэкапов; бэкап без остановки сервиса.

- [ ] **Step 1:** Бэкап-скрипт с проверкой целостности (`PRAGMA integrity_check`) и ненулевого размера.
- [ ] **Step 2:** Проверить восстановление: развернуть бэкап в чистый том, поднять сервер, `GET /v1/usage` показывает прежние цифры.
- [ ] **Step 3:** Описать обновление: `docker compose pull && docker compose up -d`, откат — предыдущий тег образа; предупредить, что формат индекса меняется только с миграцией.
- [ ] **Step 4:** Проверить квоты из контейнера: `docker compose exec pms-server pms-server quota-set --config /etc/pms/server.toml --account <hex> --bytes …`.
- [ ] **Step 5:** Commit.

### Task 5: Проверка на чистой VM

**Files:** (вне репозитория) тестовая VM `192.168.1.233`.

- [ ] **Step 1:** Убрать systemd-развёртывание прототипа (остановить юнит, оставить бинарник), освободить порт 8443.
- [ ] **Step 2:** Установить Docker, поднять compose, проверить `healthz`, `usage`, регистрацию с ноутбука и с телефона.
- [ ] **Step 3:** Замер: RSS контейнера, размер образа, время холодного старта, время сборки образа на 2 vCPU.
- [ ] **Step 4:** Записать цифры в `deploy/docker/README.md` и в отчёт `docs/reports/…-docker.md`.

## Self-Review

Оба режима TLS входят в план, потому что пользователь просил оба и оба с автопродлением. Цена режима `acme` — TLS терминирует Caddy, значит внутри compose `pms-server` работает без TLS; это записано в README рядом с конфигом, и порт наружу публикует только Caddy. Мониторинг и алерты (Prometheus, uptime) вне плана: одна VM, healthcheck и journal достаточно. Мониторинг и алерты (Prometheus, uptime) вне плана: одна VM, healthcheck + journal достаточно.
