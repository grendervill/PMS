# Развёртывание `pms-server`

Сервер хранит запечатанный сейф и запечатанные вложения. Ключей у него нет:
ни мастер-ключа, ни ключа сейфа. Скомпрометированный сервер отдаёт только шифротекст.

## 1. Сборка

```bash
# На сервере (Debian/Ubuntu): нужен cmake для aws-lc-rs
apt-get install -y build-essential cmake pkg-config curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
cargo build --profile server -p pms-server
install -m 0755 target/server/pms-server /usr/local/bin/pms-server
```

## 2. Пользователь и каталоги

```bash
useradd --system --home /var/lib/pms --shell /usr/sbin/nologin pms
install -d -o pms -g pms -m 0700 /var/lib/pms
install -d -m 0755 /etc/pms
```

## 3. Сертификат

Публично доверенный (Let's Encrypt) — сервер в интернете:

```bash
apt-get install -y certbot
certbot certonly --standalone -d pms.example.com
# /etc/letsencrypt/live/pms.example.com/{fullchain.pem,privkey.pem}
```

Свой CA или самоподписанный — домашняя сеть. **SAN обязателен**: rustls игнорирует CN.

```bash
openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
  -keyout /etc/pms/server.key -out /etc/pms/server.crt \
  -subj "/CN=pms.home" \
  -addext "subjectAltName=DNS:pms.home,IP:192.168.1.10"
chown root:pms /etc/pms/server.crt /etc/pms/server.key
chmod 0644 /etc/pms/server.crt
chmod 0640 /etc/pms/server.key
```

Если оставить ключ `0600 root:root`, сервис под пользователем `pms` не стартует —
`check-config` от root при этом проходит, поэтому проверять надо тем же пользователем,
что и сервис (или сразу `systemctl start`).

Клиент в таком случае подключается с `tls_policy = "pin"` и отпечатком:

```bash
openssl x509 -in /etc/pms/server.crt -noout -fingerprint -sha256
```

## 4. Конфиг и запуск

```bash
install -m 0644 crates/pms-server/pms-server.example.toml /etc/pms/server.toml
$EDITOR /etc/pms/server.toml        # listen, data_dir, пути к сертификату
install -m 0644 deploy/pms-server.service /etc/systemd/system/pms-server.service
pms-server check-config --config /etc/pms/server.toml   # проверка до рестарта
systemctl daemon-reload
systemctl enable --now pms-server
journalctl -u pms-server -n 20
```

Проверка живости:

```bash
curl --cacert /etc/pms/server.crt https://pms.home:8443/v1/healthz
```

## 4a. Что стоит выставить сразу

- `limits.max_body_bytes` — не меньше 64 КиБ (меньше сервер отвергнет на старте: иначе
  перестанут проходить сами запросы входа). Разумно 64–512 МиБ.
- `limits.body_timeout_secs` — потолок на приём одного тела; защищает от «медленных» загрузок.
- `registration.token` — если сервер смотрит в интернет, закройте регистрацию токеном.
- `registration.max_per_minute` — по умолчанию 30 регистраций в минуту на весь сервер.
- Per-IP ограничения ставьте на reverse proxy (Caddy/nginx): сервер за прокси не может
  доверять `X-Forwarded-For`, поэтому сам их не делает.

## 5. Квоты

Дефолт — 1 ГиБ на аккаунт (`quota.default_bytes`). Индивидуально:

```bash
pms-server quota-set --config /etc/pms/server.toml --account <32 hex> --bytes 2147483648
```

## 6. Бэкап

Индекс и байты живут раздельно: `index.sqlite` (WAL) и каталоги шардов
`<первые 2 hex>/<account_id>/` с `vault.bin` и `blobs/`.

```bash
sqlite3 /var/lib/pms/index.sqlite ".backup '/backup/pms-index.sqlite'"
rsync -a --delete --exclude certs --exclude 'index.sqlite*' /var/lib/pms/ /backup/pms-data/
```

Без `index.sqlite` сервер при старте пересоберёт `used_bytes` по файлам, но список
аккаунтов, логины и сессии потеряются — поэтому индекс в бэкапе обязателен.
Готовый скрипт для контейнера: `deploy/docker/backup.sh`.

## 7. Обновление

```bash
systemctl stop pms-server
install -m 0755 target/server/pms-server /usr/local/bin/pms-server
systemctl start pms-server
```

## 8. Результаты проверки на тестовой VM

Debian 13 (trixie), 2 vCPU, 2 ГиБ RAM, локальная сеть:

| Проверка | Результат |
|---|---|
| Сборка `cargo build --release -p pms-server` на самой VM | 2 мин 13 с |
| Размер бинарника | 13 МиБ |
| RSS в простое | 18.1 МиБ (`ps`), 0.4 МиБ в cgroup по `MemoryCurrent` |
| Каталог данных пустого сервера | 108 КиБ (пустой `index.sqlite` с WAL) |
| `GET /v1/healthz` по TLS с CA из своего сертификата | `200 {"protocol":1,"status":"ok"}` |
| Тот же запрос без доверия к сертификату | обрыв на рукопожатии |
| `http://` на TLS-порт | обрыв (`record layer failure`), открытого порта нет |
| `check-config` без сертификата | отказ с кодом 1 и текстом причины |
| `cargo test -p pms-server` на Linux | 57 тестов зелёные |
| Стриминг: разовая аллокация при 16 МиБ тела | ≤ 1 МиБ (`tests/streaming.rs`) |

Что осталось на живых устройствах: полный сценарий «ноутбук + телефон» через приложение —
этап плана 2 (клиентский транспорт) и плана 5 (Android).
