# PMS-сервер на Docker Compose

Один VPS, один сервер, много пользователей. Данные и сертификаты — в томе,
конфиг — в образе. TLS обязателен: без сертификата контейнер не стартует.

## Быстрый старт

```bash
git clone https://github.com/grendervill/PMS && cd PMS/deploy/docker
cp .env.example .env          # домен/IP и режим TLS
docker compose up -d --build
```

Проверка:

```bash
docker compose ps                                # healthy
docker compose exec -T pms-server sh -c \
  'openssl s_client -connect 127.0.0.1:8443 </dev/null 2>/dev/null | head -3'
```

## Два режима TLS

Выбираются в `.env` (`PMS_TLS`), оба продлеваются автоматически.

### `self-signed` (по умолчанию, домашняя сеть)

Сервер держит TLS сам. При первом старте `entrypoint.sh` выпускает сертификат
через openssl: RSA 3072, срок 3650 дней, SAN из `PMS_DOMAIN` и `PMS_IP`
(без SAN проверка имени не пройдёт — rustls игнорирует CN).

Продление — cron на хосте, за 30 дней до истечения, и только при успехе:

```cron
0 4 * * * /opt/pms/deploy/docker/renew-self-signed.sh >> /var/log/pms-cert.log 2>&1
```

Клиент подключается с политикой `pin`: приложение показывает отпечаток ключа,
его надо сверить с сервером:

```bash
openssl x509 -in <(docker compose exec -T pms-server cat /var/lib/pms/certs/server.crt) \
  -noout -pubkey | openssl pkey -pubin -outform der | sha256sum
```

### `acme` (сервер смотрит в интернет)

```bash
PMS_TLS=acme ACME_EMAIL=you@example.com docker compose --profile acme up -d
```

TLS терминирует Caddy (`Caddyfile`, Let's Encrypt, автопродление), а `pms-server`
слушает только внутреннюю сеть compose. Это ослабляет fail-closed внутри
контейнера, поэтому порт сервера наружу не публикуется: в `.env` поставьте
`PMS_BIND=127.0.0.1`, а 80/443 держит Caddy.

## Данные

Всё в томе `pms_pms-data` → `/var/lib/pms`:

```
index.sqlite              метаданные: аккаунты, логины, квоты, сессии
<2 hex>/<account_id>/     vault.bin и blobs/ запечатанных данных
certs/                    сертификат и ключ (в бэкап не попадают)
staging/                  временные загрузки
```

Квоты по умолчанию 1 ГиБ на аккаунт; менять адресно:

```bash
docker compose exec -T pms-server pms-server quota-set \
  --config /etc/pms/server.toml --account <32 hex> --bytes 2147483648
docker compose exec -T pms-server pms-server accounts --config /etc/pms/server.toml
```

## Бэкап и восстановление

```bash
./backup.sh /root/pms-backups
```

Скрипт делает согласованный снимок SQLite через `.backup` (работает при активной
записи), проверяет `PRAGMA integrity_check` и архивирует каталоги аккаунтов.
Сертификаты не архивируются.

Восстановление — **при остановленном сервере**, иначе WAL старого тома
переиграет восстановленную базу:

```bash
docker compose stop pms-server
docker compose run --rm --entrypoint sh pms-server \
  -c 'rm -f /var/lib/pms/index.sqlite /var/lib/pms/index.sqlite-wal /var/lib/pms/index.sqlite-shm'
cat index-<stamp>.sqlite | docker compose run --rm -T --entrypoint sh pms-server \
  -c 'cat > /var/lib/pms/index.sqlite'
cat accounts-<stamp>.tar.gz | docker compose run --rm -T --entrypoint tar pms-server \
  -xzf - -C /var/lib/pms
docker compose start pms-server
```

## Обновление и откат

```bash
git pull
docker compose build && docker compose up -d
```

Откат — прошлый тег: `PMS_TAG=<старый> docker compose up -d`. Формат индекса
меняется только вместе с миграцией в `store/sqlite.rs`; при откате на версию до
миграции сервер не стартует — тогда верните бэкап.

## Замеры на тестовой VM (Debian 13, 2 vCPU, 2 ГиБ)

| Проверка | Результат |
|---|---|
| Образ | 96.3 МБ (runtime — debian-slim + openssl/sqlite3) |
| Память контейнера в работе | 9.5 МиБ из лимита 256 МиБ |
| Холодный старт до `healthy` | ~10 с |
| Сертификат при первом старте | выпущен, `DNS:pms.test, IP:192.168.1.233`, 10 лет |
| `curl --cacert` к `/v1/healthz` | `200 {"protocol":1,"status":"ok"}` |
| Регистрация и пуш сейфа через контейнер | аккаунт создан, 15 байт занято |
| `backup.sh` | снимок + архив, `integrity_check` = ok |
| Восстановление в чистый том | оба аккаунта на месте, `healthz` 200 |

ACME-режим на этой VM не проверялся: нужен публичный домен и открытые 80/443.
