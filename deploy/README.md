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
chmod 0600 /etc/pms/server.key
```

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

## 5. Квоты

Дефолт — 1 ГиБ на аккаунт (`quota.default_bytes`). Индивидуально:

```bash
pms-server quota-set --config /etc/pms/server.toml --account <32 hex> --bytes 2147483648
```

## 6. Бэкап

Индекс и байты живут раздельно: `index.sqlite` (WAL) и каталоги `accounts/`.

```bash
sqlite3 /var/lib/pms/index.sqlite ".backup '/backup/pms-index.sqlite'"
rsync -a --delete /var/lib/pms/accounts/ /backup/pms-accounts/
```

Без `index.sqlite` сервер при старте пересоберёт `used_bytes` по файлам, но список
блобов и аккаунтов потеряется — поэтому индекс в бэкапе обязателен.

## 7. Обновление

```bash
systemctl stop pms-server
install -m 0755 target/server/pms-server /usr/local/bin/pms-server
systemctl start pms-server
```

## 8. Результаты проверки на тестовой VM

Заполняется после прогона (этап 13 плана).
