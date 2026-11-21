#!/bin/sh
# Продление самоподписанного сертификата по cron.
#
#  0 4 * * *  /opt/pms/deploy/docker/renew-self-signed.sh >> /var/log/pms-cert.log 2>&1
#
# Перевыпуск только за 30 дней до конца, и рестарт — только при успехе:
# неудачное продление не должно оставить сервер без рабочего сертификата.
set -eu

threshold_days="${PMS_RENEW_DAYS:-30}"
cert="/etc/pms/server.crt"

needs_renewal() {
    docker compose exec -T pms-server sh -c \
        "openssl x509 -in $cert -checkend $((threshold_days * 86400)) >/dev/null 2>&1 && exit 1 || exit 0" \
        >/dev/null 2>&1
}

if ! needs_renewal; then
    echo "сертификат ещё действителен, продление не нужно"
    exit 0
fi

echo "перевыпускаю сертификат"
docker compose exec -T pms-server rm -f "$cert" /etc/pms/server.key
if docker compose restart pms-server; then
    echo "сертификат перевыпущен, сервис перезапущен"
else
    echo "перезапуск не удался, проверьте docker compose logs pms-server" >&2
    exit 1
fi
