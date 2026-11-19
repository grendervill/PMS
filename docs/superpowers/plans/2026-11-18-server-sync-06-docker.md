# PMS Server — Implementation Plan 6: Docker Compose для развёртывания

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сервер поднимается одной командой `docker compose up -d` на чистом VPS: образ собирается из репозитория, данные и сертификаты живут в томах, обновление — смена тега и `docker compose up -d`.

**Architecture:** Multi-stage образ: сборка в `rust:1-slim` (нужен `cmake` для `aws-lc-rs`, `libsqlite3` не нужен — `rusqlite` собирается bundled), рантайм — `debian:bookworm-slim` с `ca-certificates` и `tzdata` и непривилегированным пользователем. `pms-server` остаётся **единственным**, кто терминирует TLS: сертификаты монтируются в `/etc/pms`, `tls.required = true` сохраняется. Certbot — отдельный контейнер-сайдкар, который кладёт сертификаты в общий том и перезапускает сервер по хуку. Compose описывает только то, что нужно одному VPS: ресурсы, healthcheck, ротацию логов, бэкап-скрипт.

**Tech Stack:** Docker Engine 27+, Docker Compose v2, BuildKit (multi-arch `linux/amd64` + `linux/arm64`), certbot, healthcheck через `curl /v1/healthz`, cron/systemd-timer для бэкапа.

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
- ACME не встраивается в `pms-server` (§11): сертификаты выпускает certbot или ставятся вручную.

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

### Task 2: Compose-файл

**Files:** Create `deploy/docker/compose.yaml`, `deploy/docker/server.toml`, `deploy/docker/.env.example`.

**Interfaces:**
- Сервис `pms-server`: образ, `ports: ["8443:8443"]`, тома `pms-data:/var/lib/pms` и `pms-certs:/etc/pms:ro`, `read_only: true`, `tmpfs: /tmp`, `user: "10001:10001"`, `mem_limit: 256m`, `restart: unless-stopped`, `healthcheck` (`curl -fsS --cacert /etc/pms/server.crt https://127.0.0.1:8443/v1/healthz`), логи `json-file` с ротацией 10 МБ × 3.
- Сервис `certbot` (профиль `acme`): образ `certbot/certbot`, том `pms-certs`, `entrypoint` с `certonly --standalone` + `trap` на продление, deploy-hook `docker compose restart pms-server`.

- [ ] **Step 1:** Описать compose и конфиг; в `server.toml` — `tls.required = true`, `mode = "files"`, пути в `/etc/pms`, `quota.default_bytes = 1073741824`.
- [ ] **Step 2:** Проверить с самоподписанным сертификатом в томе: `openssl req … -addext "subjectAltName=…"` (SAN обязателен), `docker compose up -d`, `curl --cacert` получает `{"protocol":1,"status":"ok"}`.
- [ ] **Step 3:** Проверить отказ: без сертификата в томе контейнер уходит в `Restarting` с понятным логом; `docker compose ps` это показывает.
- [ ] **Step 4:** Commit.

### Task 3: Данные, бэкап и обновление

**Files:** Create `deploy/docker/backup.sh`, `deploy/docker/compose.backup.yaml`, `deploy/docker/README.md`; Modify `deploy/README.md` (ссылка на compose-вариант).

**Interfaces:** `backup.sh` → `sqlite3 /var/lib/pms/index.sqlite ".backup …"` внутри контейнера + `tar` каталога `accounts/` в примонтированный каталог бэкапов; бэкап без остановки сервиса.

- [ ] **Step 1:** Бэкап-скрипт с проверкой целостности (`PRAGMA integrity_check`) и ненулевого размера.
- [ ] **Step 2:** Проверить восстановление: развернуть бэкап в чистый том, поднять сервер, `GET /v1/usage` показывает прежние цифры.
- [ ] **Step 3:** Описать обновление: `docker compose pull && docker compose up -d`, откат — предыдущий тег образа; предупредить, что формат индекса меняется только с миграцией.
- [ ] **Step 4:** Проверить квоты из контейнера: `docker compose exec pms-server pms-server quota-set --config /etc/pms/server.toml --account <hex> --bytes …`.
- [ ] **Step 5:** Commit.

### Task 4: Проверка на чистой VM

**Files:** (вне репозитория) тестовая VM `192.168.1.233`.

- [ ] **Step 1:** Убрать systemd-развёртывание прототипа (остановить юнит, оставить бинарник), освободить порт 8443.
- [ ] **Step 2:** Установить Docker, поднять compose, проверить `healthz`, `usage`, регистрацию с ноутбука и с телефона.
- [ ] **Step 3:** Замер: RSS контейнера, размер образа, время холодного старта, время сборки образа на 2 vCPU.
- [ ] **Step 4:** Записать цифры в `deploy/docker/README.md` и в отчёт `docs/reports/…-docker.md`.

## Self-Review

Reverse proxy с ACME (Caddy) как альтернатива certbot описан в README, но не входит в compose: `pms-server` в таком варианте обязан работать с `tls.required = false`, а это ослабляет fail-closed — выбор оставлен деплойменту, а не умолчанию. Мониторинг и алерты (Prometheus, uptime) вне плана: одна VM, healthcheck + journal достаточно.
