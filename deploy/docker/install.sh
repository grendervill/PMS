#!/bin/sh
# Установка PMS-сервера на чистый Debian/Ubuntu VPS.
#
#   curl -fsSL https://raw.githubusercontent.com/grendervill/PMS/main/deploy/docker/install.sh | sh
#
# Ставит Docker, копирует compose-файлы в /opt/pms и запускает сервер.
set -eu

root="${PMS_DIR:-/opt/pms}"

if ! command -v docker >/dev/null 2>&1; then
    echo "ставлю Docker"
    curl -fsSL https://get.docker.com | sh
fi

if ! docker compose version >/dev/null 2>&1; then
    echo "нужен docker compose plugin" >&2
    exit 1
fi

mkdir -p "$root/deploy/docker"
cd "$root"

if [ ! -f deploy/docker/compose.yaml ]; then
    echo "склонируйте репозиторий в $root и запустите install.sh оттуда" >&2
    exit 1
fi

if [ ! -f deploy/docker/.env ]; then
    cp deploy/docker/.env.example deploy/docker/.env
    echo "готово: поправьте deploy/docker/.env (домен, IP, режим TLS)"
fi

cd deploy/docker
docker compose up -d --build
docker compose ps
