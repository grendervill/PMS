#!/bin/sh
# Готовит каталог сертификатов и, если нужно, выпускает самоподписанный.
#
# ACME-вариант (Caddy) сюда не заходит: там сертификатом занимается прокси,
# а сервер запускается с tls.required = false на внутреннем адресе.
set -eu

CERT=/etc/pms/server.crt
KEY=/etc/pms/server.key

if [ "${PMS_TLS:-self-signed}" = "self-signed" ] && [ ! -s "$CERT" ]; then
    days="${PMS_CERT_DAYS:-3650}"
    name="${PMS_DOMAIN:-pms.home}"
    ip="${PMS_IP:-}"

    alt="DNS:$name"
    if [ -n "$ip" ]; then
        alt="$alt,IP:$ip"
    fi

    echo "pms: выпускаю самоподписанный сертификат на $days дней ($alt)"
    openssl req -x509 -newkey rsa:3072 -sha256 -days "$days" -nodes \
        -keyout "$KEY" -out "$CERT" \
        -subj "/CN=$name" \
        -addext "subjectAltName=$alt" >/dev/null 2>&1
    chmod 0600 "$KEY"
    chmod 0644 "$CERT"
fi

if [ "${PMS_TLS:-self-signed}" = "self-signed" ] && [ ! -s "$CERT" ]; then
    echo "pms: сертификата нет, а tls.required = true — не стартую" >&2
    exit 1
fi

exec pms-server "$@"
