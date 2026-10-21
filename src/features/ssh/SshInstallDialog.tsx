import { AlertTriangle, CheckCircle2, FolderOpen, KeySquare, Loader2, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { homeDir, join } from "@tauri-apps/api/path";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Modal } from "@/components/ui/Modal";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { errorCode, errorText } from "@/features/vault/errors";
import {
  vaultApi,
  type InstalledSshKey,
  type SshPermissionStatus,
} from "@/features/vault/api";
import type { SshKey } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";

function safeName(value: string) {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9._-]/g, "")
      .replace(/^[-.]+/, "") || "id_key"
  );
}

function mode(value?: number | null) {
  return value === undefined || value === null ? "—" : value.toString(8).padStart(3, "0");
}

export function SshInstallDialog({
  sshKey,
  onClose,
}: {
  sshKey: SshKey;
  onClose: () => void;
}) {
  const confirm = useConfirm();
  const toast = useToast();
  const [directory, setDirectory] = useState("");
  const [name, setName] = useState(safeName(sshKey.name));
  const [busy, setBusy] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [installed, setInstalled] = useState<InstalledSshKey | null>(null);
  const [status, setStatus] = useState<SshPermissionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const resolveDirectory = async () => {
    if (directory.trim()) return directory.trim();
    const resolved = await join(await homeDir(), ".ssh");
    setDirectory(resolved);
    return resolved;
  };

  const check = async (target: string) => {
    try {
      setStatus(await vaultApi.sshCheck(target, name.trim()));
    } catch {
      setStatus(null);
    }
  };

  const runInstall = async (target: string, allowOverwrite: boolean) => {
    try {
      const result = await vaultApi.sshInstall(
        target,
        name.trim(),
        sshKey.privateKey,
        sshKey.publicKey,
        allowOverwrite,
      );
      setInstalled(result);
      await check(target);
      toast.show({
        title: "Ключ установлен",
        description: `${result.privatePath} · права ${mode(result.privateMode)}`,
        tone: "success",
      });
    } catch (installError) {
      if (errorCode(installError) === "file_exists" && !allowOverwrite) {
        const allowed = await confirm({
          title: "Файл уже существует",
          description: "Перезаписать существующий SSH-ключ? Старый файл будет заменён.",
          confirmLabel: "Перезаписать",
          tone: "danger",
        });
        if (allowed) await runInstall(target, true);
        return;
      }
      setError(errorText(installError));
    }
  };

  const install = async () => {
    const clean = name.trim();
    if (!/^[A-Za-z0-9._-]+$/.test(clean) || clean === "." || clean === "..") {
      setError("Имя файла: только буквы, цифры, точка, дефис и подчёркивание");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await runInstall(await resolveDirectory(), false);
    } catch (resolveError) {
      setError(errorText(resolveError));
    } finally {
      setBusy(false);
    }
  };

  const chooseFolder = async () => {
    setChoosing(true);
    try {
      const selected = await getStorage().pickFolder();
      if (selected) setDirectory(selected);
    } finally {
      setChoosing(false);
    }
  };

  const checkAgain = async () => {
    if (!directory.trim()) return;
    await check(directory.trim());
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Установить SSH-ключ"
      description="Приватный ключ будет записан на диск. Убедитесь, что папка предназначена для SSH."
      className="w-[min(92vw,600px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Закрыть
          </Button>
          <Button onClick={() => void install()} disabled={busy || choosing}>
            {busy ? <Loader2 size={15} className="animate-spin" /> : <KeySquare size={15} />}
            {installed ? "Установить снова" : "Установить"}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <div className="rounded-[var(--radius-sm)] border border-[var(--warning)]/25 bg-[var(--warning)]/5 px-3 py-2.5 text-[11px] leading-relaxed text-[var(--text-soft)]">
          Файлы получат права <span className="pms-mono">600</span> для приватного ключа,{" "}
          <span className="pms-mono">644</span> для публичного и <span className="pms-mono">700</span>{" "}
          для папки. Права чужих файлов не меняются автоматически.
        </div>

        <Field
          label="Папка установки"
          htmlFor="ssh-install-directory"
          hint="Если оставить пустой, будет использована ~/.ssh."
        >
          <div className="flex gap-2">
            <Input
              id="ssh-install-directory"
              value={directory}
              onChange={(event) => setDirectory(event.target.value)}
              placeholder="~/.ssh"
              className="pms-mono text-[12px]"
            />
            <Button
              variant="secondary"
              size="md"
              onClick={() => void chooseFolder()}
              disabled={busy || choosing}
            >
              {choosing ? <Loader2 size={15} className="animate-spin" /> : <FolderOpen size={15} />}
              Выбрать
            </Button>
          </div>
        </Field>

        <Field
          label="Имя файла"
          htmlFor="ssh-install-name"
          hint="Будут созданы файлы name и name.pub."
        >
          <Input
            id="ssh-install-name"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
              setInstalled(null);
              setStatus(null);
            }}
            placeholder="id_ed25519"
            className="pms-mono text-[12px]"
          />
        </Field>

        {installed ? (
          <div className="grid gap-2 rounded-[var(--radius-sm)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 p-3">
            <p className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--accent-soft)]">
              <CheckCircle2 size={14} />
              Файлы установлены
            </p>
            <div className="grid gap-1 text-[11px] text-[var(--text-soft)]">
              <span className="pms-mono break-all">{installed.privatePath} · {mode(installed.privateMode)}</span>
              <span className="pms-mono break-all">{installed.publicPath} · {mode(installed.publicMode)}</span>
              <span className="pms-mono break-all">{directory} · {mode(installed.directoryMode)}</span>
            </div>
            {installed.warning ? (
              <p className="flex items-start gap-1.5 text-[10.5px] leading-relaxed text-[var(--warning)]">
                <AlertTriangle size={12} className="mt-0.5 shrink-0" />
                {installed.warning}
              </p>
            ) : null}
            {status ? (
              <div className="flex items-center gap-2 text-[10.5px]">
                <ShieldCheck
                  size={12}
                  className={status.ok ? "text-[var(--accent-soft)]" : "text-[var(--warning)]"}
                />
                <span className={status.ok ? "text-[var(--accent-soft)]" : "text-[var(--warning)]"}>
                  {status.ok ? "Права в порядке" : "Права требуют внимания"}
                </span>
                <button
                  type="button"
                  onClick={() => void checkAgain()}
                  className="text-[var(--muted)] underline decoration-dotted hover:text-[var(--text)]"
                >
                  Проверить снова
                </button>
              </div>
            ) : null}
            {status && !status.ok && status.exists ? (
              <p className="text-[10px] leading-relaxed text-[var(--muted)]">
                Ожидается: папка 700, приватный ключ 600, публичный 644. Владелец:{" "}
                {status.ownedByCurrentUser ? "вы" : "другой пользователь"}.
              </p>
            ) : null}
          </div>
        ) : null}

        {error ? (
          <p role="alert" className="text-[11.5px] text-[var(--danger)]">
            {error}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
