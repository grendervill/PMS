import { useState } from "react";
import { Check, Copy, Globe, Lock, Terminal } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { copySecret } from "@/lib/clipboard";

type CertMode = "acme" | "self-signed";

// Пока нет готового образа (план 6), это ровно те шаги, что описаны в
// deploy/README.md и проверены на тестовой VM.
const INSTALL = `apt-get install -y build-essential cmake curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
git clone https://github.com/grendervill/PMS && cd PMS
cargo build --profile server -p pms-server
install -m 0755 target/server/pms-server /usr/local/bin/pms-server

pms-server check-config --config /etc/pms/server.toml`;

const SELF_SIGNED = `openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \\
  -keyout /etc/pms/server.key -out /etc/pms/server.crt \\
  -subj "/CN=pms.home" \\
  -addext "subjectAltName=DNS:pms.home,IP=192.168.1.10"

# ключ читается только сервисом
chown root:pms /etc/pms/server.key
chmod 0640 /etc/pms/server.key`;

const ACME = `# Caddy сам получает и продлевает сертификат
pms.home {
    reverse_proxy 127.0.0.1:8443
}

# в [tls] сервера: required = false, только внутренний адрес
systemctl restart pms-server`;

/**
 * Развёртывание и сертификат. Приложение сертификаты не выпускает — оно даёт
 * команды и объясняет, что выбрать, а проверяет уже подключение.
 */
export function ServerSetupScreen({
  onContinue,
  onCancel,
}: {
  onContinue: () => void;
  onCancel: () => void;
}) {
  const [mode, setMode] = useState<CertMode>("self-signed");
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (label: string, value: string) => {
    try {
      // Команды — не секреты: чистить буфер не нужно.
      await copySecret(value, { clearAfterMs: 0 });
      setCopied(label);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // Копирование — удобство, а не обязательный шаг.
    }
  };

  return (
    <main className="relative min-h-screen overflow-hidden bg-[var(--bg)] text-[var(--text)]">
      <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col justify-center px-6 py-12 sm:px-10">
        <button
          type="button"
          onClick={onCancel}
          className="mb-10 self-start text-[11.5px] text-[var(--muted)] transition-colors hover:text-[var(--text-soft)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
        >
          ← Пропустить настройку
        </button>

        <h1 className="pms-display text-[34px] leading-[1.1] tracking-tight">Развёртывание сервера</h1>
        <p className="mt-3 max-w-lg text-[13.5px] leading-relaxed text-[var(--muted)]">
          Сборка и установка на своём VPS. Конфиг и systemd-юнит лежат в
          <span className="pms-mono text-[11.5px] text-[var(--text-soft)]">deploy/</span>; образ и
          compose появятся отдельным этапом.
        </p>

        <section className="mt-8 border-t border-[var(--border)] pt-6">
          <div className="flex items-center gap-2 text-[11px] text-[var(--muted)]">
            <Terminal size={14} className="text-[var(--accent-soft)]" />
            Шаг 1 · установка
          </div>
          <pre className="pms-mono mt-3 overflow-x-auto rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/70 px-4 py-3 text-[11.5px] leading-relaxed text-[var(--text-soft)]">
            {INSTALL}
          </pre>
          <Button
            size="sm"
            variant="secondary"
            className="mt-2"
            onClick={() => void copy("install", INSTALL)}
          >
            {copied === "install" ? <Check size={14} /> : <Copy size={14} />}
            {copied === "install" ? "Скопировано" : "Скопировать"}
          </Button>
        </section>

        <section className="mt-8 border-t border-[var(--border)] pt-6">
          <div className="flex items-center gap-2 text-[11px] text-[var(--muted)]">
            <Lock size={14} className="text-[var(--accent-soft)]" />
            Шаг 2 · сертификат
          </div>

          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            <button
              type="button"
              onClick={() => setMode("self-signed")}
              aria-pressed={mode === "self-signed"}
              className={`rounded-[var(--radius-md)] border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${
                mode === "self-signed"
                  ? "border-[var(--accent)] bg-[var(--accent)]/10"
                  : "border-[var(--border)] bg-[var(--surface)]/60 hover:border-[var(--border-strong)]"
              }`}
            >
              <p className="text-[13px] font-medium text-[var(--text)]">Самоподписанный</p>
              <p className="mt-1 text-[11px] leading-relaxed text-[var(--muted)]">
                10 лет, свой openssl, продление по cron. Для домашней сети.
              </p>
            </button>
            <button
              type="button"
              onClick={() => setMode("acme")}
              aria-pressed={mode === "acme"}
              className={`rounded-[var(--radius-md)] border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${
                mode === "acme"
                  ? "border-[var(--accent)] bg-[var(--accent)]/10"
                  : "border-[var(--border)] bg-[var(--surface)]/60 hover:border-[var(--border-strong)]"
              }`}
            >
              <p className="text-[13px] font-medium text-[var(--text)]">Let's Encrypt</p>
              <p className="mt-1 text-[11px] leading-relaxed text-[var(--muted)]">
                Через Caddy, продление автоматическое. Если сервер смотрит в интернет.
              </p>
            </button>
          </div>

          <pre className="pms-mono mt-3 overflow-x-auto rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/70 px-4 py-3 text-[11.5px] leading-relaxed text-[var(--text-soft)]">
            {mode === "self-signed" ? SELF_SIGNED : ACME}
          </pre>
          <Button
            size="sm"
            variant="secondary"
            className="mt-2"
            onClick={() => void copy("cert", mode === "self-signed" ? SELF_SIGNED : ACME)}
          >
            {copied === "cert" ? <Check size={14} /> : <Copy size={14} />}
            {copied === "cert" ? "Скопировано" : "Скопировать"}
          </Button>
        </section>

        <section className="mt-8 border-t border-[var(--border)] pt-6">
          <div className="flex items-center gap-2 text-[11px] text-[var(--muted)]">
            <Globe size={14} className="text-[var(--accent-soft)]" />
            Шаг 3 · подключение
          </div>
          <p className="mt-2 max-w-lg text-[12.5px] leading-relaxed text-[var(--text-soft)]">
            Когда сервер отвечает, вернитесь сюда: приложение проверит адрес, покажет
            отпечаток ключа и попросит логин с мастер-паролем.
          </p>
          <Button size="md" className="mt-4 h-10 px-4" onClick={onContinue}>
            Сервер готов — подключиться
          </Button>
        </section>
      </div>
    </main>
  );
}
