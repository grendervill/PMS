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

# Сначала данные, потом индекс: при восстановлении `recount` считает файл
# истиной и сам поправит дайджест, если индекс отстал на одну запись.
# Обратный порядок оставил бы в бэкапе индекс новее данных.
# Сертификаты и файлы SQLite не архивируем: первые перевыпускаются, вторые
# снимаются отдельно через `.backup`.
docker exec "$container" tar -czf - \
    --exclude=certs --exclude='index.sqlite*' --exclude='.backup.sqlite' \
    -C /var/lib/pms . > "$target/accounts-$stamp.tar.gz"

# .backup делает согласованный снимок даже при активной записи (WAL).
docker exec "$container" sh -c \
    'sqlite3 /var/lib/pms/index.sqlite ".backup /var/lib/pms/.backup.sqlite" && sqlite3 /var/lib/pms/.backup.sqlite "PRAGMA integrity_check;" | grep -qx ok'
docker cp "$container:/var/lib/pms/.backup.sqlite" "$target/index-$stamp.sqlite"
docker exec "$container" rm -f /var/lib/pms/.backup.sqlite

[ -s "$target/accounts-$stamp.tar.gz" ] || { echo "пустой архив аккаунтов" >&2; exit 1; }
tar -tzf "$target/accounts-$stamp.tar.gz" >/dev/null 2>&1 || { echo "архив не читается" >&2; exit 1; }
[ -s "$target/index-$stamp.sqlite" ] || { echo "пустая база" >&2; exit 1; }

echo "готово:"
echo "  $target/index-$stamp.sqlite"
echo "  $target/accounts-$stamp.tar.gz"
