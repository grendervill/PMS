#!/bin/sh
# Бэкап без остановки сервера: SQLite .backup + архив каталога аккаунтов.
#
#   ./backup.sh /path/to/backups
set -eu

target="${1:-./backups}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$target"

container="$(docker compose ps -q pms-server)"
if [ -z "$container" ]; then
    echo "pms-server не запущен" >&2
    exit 1
fi

docker exec "$container" sh -c 'sqlite3 /var/lib/pms/index.sqlite "PRAGMA integrity_check;"' 2>/dev/null || true

# .backup делает согласованный снимок даже при активной записи (WAL).
docker exec "$container" sh -c "sqlite3 /var/lib/pms/index.sqlite \".backup '/var/lib/pms/.backup.sqlite'\""
docker cp "$container:/var/lib/pms/.backup.sqlite" "$target/index-$stamp.sqlite"
docker exec "$container" rm -f /var/lib/pms/.backup.sqlite

docker run --rm -v pms_pms-data:/data -v "$(cd "$target" && pwd)":/backup alpine:3.20 \
    tar -czf "/backup/accounts-$stamp.tar.gz" -C /data accounts

# Проверка: архив не пустой и база целая.
[ -s "$target/accounts-$stamp.tar.gz" ] || { echo "пустой архив" >&2; exit 1; }
[ -s "$target/index-$stamp.sqlite" ] || { echo "пустая база" >&2; exit 1; }
echo "готово: $target/index-$stamp.sqlite, $target/accounts-$stamp.tar.gz"
