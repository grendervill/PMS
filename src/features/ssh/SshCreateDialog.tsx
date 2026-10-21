import { KeySquare, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { useToast } from "@/components/ui/toast-context";
import { vaultApi, type GeneratedSshKey, type SshGenerateOptions } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { SshKeyType } from "@/features/vault/types";

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

export function SshCreateDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated?: (id: string) => void;
}) {
  const addSshKey = useVaultStore((state) => state.addSshKey);
  const toast = useToast();
  const [name, setName] = useState("");
  const [keyType, setKeyType] = useState<SshKeyType>("ed25519");
  const [rsaBits, setRsaBits] = useState(3072);
  const [curve, setCurve] = useState("p256");
  const [comment, setComment] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [generated, setGenerated] = useState<GeneratedSshKey | null>(null);
  const [generating, setGenerating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const options: SshGenerateOptions = {
    keyType,
    rsaBits: keyType === "rsa" ? rsaBits : null,
    curve: keyType === "ecdsa" ? curve : null,
    comment: comment.trim(),
    passphrase: passphrase || null,
  };

  const generate = async () => {
    setGenerating(true);
    setError(null);
    try {
      const key = await vaultApi.sshGenerate(options);
      setGenerated(key);
      if (!name.trim()) setName(safeName(comment || "id_key"));
    } catch (generateError) {
      setError(errorText(generateError));
    } finally {
      setGenerating(false);
    }
  };

  const save = async () => {
    if (!name.trim()) {
      setError("Введите имя ключа");
      return;
    }
    if (!generated) {
      setError("Сначала сгенерируйте ключ");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const ok = await addSshKey({
        name: name.trim(),
        type: generated.keyType,
        rsaBits: generated.rsaBits ?? null,
        curve: generated.curve ?? null,
        publicKey: generated.publicKey,
        fingerprint: generated.fingerprint,
        privateKey: generated.privateKey,
        passphrase: passphrase || null,
        comment: comment.trim(),
        attachmentIds: [],
      });
      if (ok) {
        toast.show({ title: "SSH-ключ создан", tone: "success" });
        const created = useVaultStore.getState().data?.sshKeys[0];
        if (created) onCreated?.(created.id);
        onClose();
      } else {
        setError(errorText(useVaultStore.getState().error));
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Новый SSH-ключ"
      description="Приватный ключ хранится в зашифрованном виде и на диск сам не попадает."
      className="w-[min(92vw,620px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            Отменить
          </Button>
          <Button variant="secondary" onClick={() => void generate()} disabled={generating || saving}>
            {generating ? (
              <Loader2 size={15} className="animate-spin" />
            ) : (
              <RefreshCw size={15} />
            )}
            Сгенерировать
          </Button>
          <Button onClick={() => void save()} disabled={!generated || saving}>
            {saving ? <Loader2 size={15} className="animate-spin" /> : <KeySquare size={15} />}
            Сохранить
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Имя ключа" htmlFor="ssh-name" hint="Например, GitHub или prod-server.">
            <Input
              id="ssh-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="GitHub"
              autoFocus
            />
          </Field>
          <Field label="Тип ключа" htmlFor="ssh-type">
            <select
              id="ssh-type"
              value={keyType}
              onChange={(event) => {
                setKeyType(event.target.value as SshKeyType);
                setGenerated(null);
              }}
              className="h-10 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
            >
              <option value="ed25519">Ed25519 · рекомендуется</option>
              <option value="rsa">RSA</option>
              <option value="ecdsa">ECDSA</option>
            </select>
          </Field>
        </div>

        {keyType === "rsa" ? (
          <Field label="Размер RSA" htmlFor="ssh-rsa-bits">
            <select
              id="ssh-rsa-bits"
              value={rsaBits}
              onChange={(event) => {
                setRsaBits(Number(event.target.value));
                setGenerated(null);
              }}
              className="h-10 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
            >
              <option value={2048}>2048 бит</option>
              <option value={3072}>3072 бита</option>
              <option value={4096}>4096 бит · медленно</option>
            </select>
          </Field>
        ) : null}

        {keyType === "ecdsa" ? (
          <Field label="Кривая ECDSA" htmlFor="ssh-curve">
            <select
              id="ssh-curve"
              value={curve}
              onChange={(event) => {
                setCurve(event.target.value);
                setGenerated(null);
              }}
              className="h-10 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
            >
              <option value="p256">P-256</option>
              <option value="p384">P-384</option>
              <option value="p521">P-521</option>
            </select>
          </Field>
        ) : null}

        <Field label="Комментарий" htmlFor="ssh-comment" hint="Комментарий попадёт в публичный ключ, например alex@mac.">
          <Input
            id="ssh-comment"
            value={comment}
            onChange={(event) => {
              setComment(event.target.value);
              setGenerated(null);
            }}
            placeholder="alex@mac"
          />
        </Field>

        <Field
          label="Парольная фраза ключа"
          htmlFor="ssh-passphrase"
          hint="Необязательно. Если задать, приватный ключ будет зашифрован; фраза сохранится в vault."
        >
          <SecretInput
            id="ssh-passphrase"
            value={passphrase}
            onChange={(event) => {
              setPassphrase(event.target.value);
              setGenerated(null);
            }}
            placeholder="Если нужна"
          />
        </Field>

        {generated ? (
          <div>
            <div className="mb-1.5 flex items-center gap-1.5 text-[11px] text-[var(--accent-soft)]">
              <ShieldCheck size={13} />
              Ключ сгенерирован
            </div>
            <div className="pms-mono max-h-32 overflow-auto break-all rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--bg)] p-3 text-[10.5px] text-[var(--text-soft)]">
              {generated.publicKey}
            </div>
            <p className="mt-1.5 text-[10px] text-[var(--muted)]">
              Fingerprint: <span className="pms-mono">{generated.fingerprint}</span>
            </p>
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
