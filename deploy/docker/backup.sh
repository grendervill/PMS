#!/bin/sh
# Бэкап без остановки сервера: согласованный снимок SQLite + архив аккаунтов.
#
#   ./backup.sh /path/to/backups
#
# Каталог данных и сертификаты лежат в одном томе, но сертификаты в бэкап не
# кладём: они перевыпускаются, а приватный ключ в архив класть незачем.
set -eu

target="${1:-./backups}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$target"

container="$(docker compose ps -q pms-server)"
if [ -z "$container" ]; then
    echo "pms-server не запущен" >&2
    exit 1
fi

# .backup делает согласованный снимок даже при активной записи (WAL).
docker exec "$container" sh -c \
    'sqlite3 /var/lib/pms/index.sqlite ".backup /var/lib/pms/.backup.sqlite" && sqlite3 /var/lib/pms/.backup.sqlite "PRAGMA integrity_check;" | grep -qx ok'
docker cp "$container:/var/lib/pms/.backup.sqlite" "$target/index-$stamp.sqlite"
docker exec "$container" rm -f /var/lib/pms/.backup.sqlite

docker exec "$container" tar -czf - -C /var/lib/pms accounts > "$target/accounts-$stamp.tar.gz"

[ -s "$target/accounts-$stamp.tar.gz" ] || { echo "пустой архив аккаунтов" >&2; exit 1; }
[ -s "$target/index-$stamp.sqlite" ] || { echo "пустая база" >&2; exit 1; }

echo "готово:"
echo "  $target/index-$stamp.sqlite"
echo "  $target/accounts-$stamp.tar.gz"
