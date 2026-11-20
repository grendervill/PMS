import { useState } from "react";
import { Loader2, Lock, ShieldAlert } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { SecretInput } from "@/components/ui/SecretInput";
import { Toggle } from "@/components/ui/Toggle";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import type { RemoteProfile, TlsPolicy } from "@/features/vault/types";
import type { ServerProfile } from "@/features/storage/profiles";

type ProbeState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ready"; fingerprint: string; tls: TlsPolicy }
  | { kind: "failed"; message: string };

type Props = {
  onConnected: (profile: ServerProfile, cacheDir: string) => void;
  onCancel: () => void;
  /** Существующий профиль: адрес и отпечаток уже известны. */
  initial?: ServerProfile;
};

function grouped(fingerprint: string): string {
  return fingerprint.replace(/(.{8})/g, "$1 ").trim().toUpperCase();
}

export function ServerProfileForm({ onConnected, onCancel, initial }: Props) {
  const [baseUrl, setBaseUrl] = useState(initial?.baseUrl ?? "");
  const [login, setLogin] = useState(initial?.login ?? "");
  const [allowHttp, setAllowHttp] = useState(initial?.tls.kind === "insecure");
  const [passphrase, setPassphrase] = useState("");
  const [probe, setProbe] = useState<ProbeState>({ kind: "idle" });
  const [busy, setBusy] = useState<"login" | "register" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const url = baseUrl.trim();
  const loginOk = /^[a-z0-9][a-z0-9._-]{2,63}$/.test(login.trim().toLowerCase());

  const check = async () => {
    if (!url) return;
    setError(null);
    setProbe({ kind: "checking" });
    const profile: RemoteProfile = {
      baseUrl: url,
      tls: allowHttp ? { kind: "insecure" } : { kind: "system" },
      login: login.trim().toLowerCase(),
    };
    try {
      const result = await vaultApi.remoteProbe(profile);
      setProbe({
        kind: "ready",
        fingerprint: result.spkiSha256,
        tls: result.spkiSha256 ? { kind: "pin", spkiSha256: result.spkiSha256 } : { kind: "insecure" },
      });
    } catch (failure) {
      setProbe({ kind: "failed", message: errorText(failure) });
    }
  };

  const connect = async (mode: "login" | "register") => {
    if (probe.kind !== "ready") return;
    setBusy(mode);
    setError(null);
    const profile: RemoteProfile = {
      baseUrl: url,
      tls: probe.tls,
      login: login.trim().toLowerCase(),
    };

    try {
      const secret = { kind: "passphrase" as const, value: passphrase };
      const session =
        mode === "login"
          ? await vaultApi.remoteLogin(profile, secret)
          : await vaultApi.remoteRegister(profile, secret);

      const id = `p-${session.accountId.slice(0, 12)}`;
      const saved: ServerProfile = {
        id,
        name: new URL(url).host,
        baseUrl: url,
        accountId: session.accountId,
        login: login.trim().toLowerCase(),
        tls: probe.tls,
        lastLoginAt: new Date().toISOString(),
      };
      const cacheDir = await vaultApi.remoteCacheDir(id);
      onConnected(saved, cacheDir);
    } catch (failure) {
      setError(errorText(failure));
    } finally {
      setBusy(null);
    }
  };

  const fingerprint = probe.kind === "ready" ? probe.fingerprint : "";

  return (
    <main className="relative min-h-screen overflow-hidden bg-[var(--bg)] text-[var(--text)]">
      <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col justify-center px-6 py-12 sm:px-10">
        <button
          type="button"
          onClick={onCancel}
          className="mb-10 self-start text-[11.5px] text-[var(--muted)] transition-colors hover:text-[var(--text-soft)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
        >
          ← Назад к выбору
        </button>

        <h1 className="pms-display text-[36px] leading-[1.08] tracking-tight">
          Подключение к серверу
        </h1>
        <p className="mt-3 max-w-md text-[13.5px] leading-relaxed text-[var(--muted)]">
          Укажите адрес своего сервера PMS. Перед входом мы покажем отпечаток его
          ключа — сверьте его с тем, что на сервере.
        </p>

        <div className="mt-9 flex flex-col gap-5 border-t border-[var(--border)] pt-6">
          <Field label="Адрес сервера" htmlFor="server-url">
            <Input
              id="server-url"
              value={baseUrl}
              onChange={(event) => {
                setBaseUrl(event.target.value);
                setProbe({ kind: "idle" });
              }}
              placeholder="https://pms.home:8443"
              autoCapitalize="off"
              spellCheck={false}
            />
          </Field>

          <div className="flex items-center justify-between gap-4">
            <p className="text-[11px] text-[var(--muted)]">
              Только для локального сервера: трафик пойдёт открытым текстом.
            </p>
            <Toggle
              checked={allowHttp}
              onCheckedChange={(value) => {
                setAllowHttp(value);
                setProbe({ kind: "idle" });
              }}
              label="Разрешить http"
            />
          </div>

          <div className="flex items-center gap-3">
            <Button
              size="md"
              variant="secondary"
              className="h-10 px-4"
              onClick={() => void check()}
              disabled={!url || probe.kind === "checking"}
            >
              {probe.kind === "checking" ? (
                <Loader2 size={15} className="animate-spin" />
              ) : (
                <Lock size={15} />
              )}
              Проверить сервер
            </Button>
            {probe.kind === "ready" ? (
              <span className="text-[11.5px] text-[var(--accent-soft)]">
                Сервер отвечает, протокол 1
              </span>
            ) : null}
          </div>

          {probe.kind === "failed" ? (
            <p
              role="alert"
              className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2.5 text-[11.5px] leading-relaxed text-[var(--danger)]"
            >
              {probe.message}
            </p>
          ) : null}

          {fingerprint ? (
            <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/70 px-4 py-3.5">
              <p className="text-[11px] text-[var(--muted)]">Отпечаток ключа (SHA-256)</p>
              <p className="pms-mono mt-2 text-[11.5px] leading-relaxed break-all tracking-wide text-[var(--text-soft)]">
                {grouped(fingerprint)}
              </p>
              <p className="mt-2.5 flex items-start gap-1.5 text-[10.5px] leading-relaxed text-[var(--warning)]">
                <ShieldAlert size={13} className="mt-0.5 shrink-0" />
                Сертификат не подтверждён публичным CA — доверие задаётся этим отпечатком.
              </p>
            </div>
          ) : null}

          <Field
            label="Логин на сервере"
            htmlFor="server-login"
            hint="3–64 символа: латиница, цифры, точка, дефис, подчёркивание"
            error={
              login.length > 0 && !loginOk
                ? "Логин: 3–64 символа, латиница, цифры, точка, дефис, подчёркивание"
                : undefined
            }
          >
            <Input
              id="server-login"
              value={login}
              onChange={(event) => setLogin(event.target.value)}
              placeholder="alice"
              autoCapitalize="off"
              spellCheck={false}
            />
          </Field>

          <Field
            label="Пароль-фраза"
            htmlFor="server-passphrase"
            hint="Тот же мастер-ключ, что открывает сейф"
          >
            <SecretInput
              id="server-passphrase"
              value={passphrase}
              onChange={(event) => setPassphrase(event.target.value)}
              placeholder="Мастер-ключ"
            />
          </Field>

          {error ? (
            <p
              role="alert"
              className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2.5 text-[11.5px] leading-relaxed text-[var(--danger)]"
            >
              {error}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center gap-2.5 border-t border-[var(--border)] pt-5">
            <Button
              size="md"
              className="h-10 px-4"
              onClick={() => void connect("login")}
              disabled={probe.kind !== "ready" || !passphrase || !loginOk || busy !== null}
            >
              {busy === "login" ? <Loader2 size={15} className="animate-spin" /> : null}
              Войти
            </Button>
            <Button
              size="md"
              variant="secondary"
              className="h-10 px-4"
              onClick={() => void connect("register")}
              disabled={probe.kind !== "ready" || !passphrase || !loginOk || busy !== null}
            >
              Создать аккаунт
            </Button>
          </div>

          <p className="text-[10.5px] leading-relaxed text-[var(--muted)]/80">
            Пароль-фраза не покидает устройство: на сервер уходит только подпись,
            сделанная ключом, который из неё выведен.
          </p>
        </div>
      </div>
    </main>
  );
}
