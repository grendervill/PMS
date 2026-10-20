import { AlertTriangle, Loader2, Save, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { Textarea } from "@/components/ui/Textarea";
import { useToast } from "@/components/ui/toast-context";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import {
  emptyEntryDraft,
  entryToDraft,
  findDuplicatePasswords,
  type EntryDraft,
} from "@/features/entries/entries";
import { useVaultStore } from "@/features/vault/store";
import type { Entry } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function EntryEditor({
  entry,
  onClose,
  onSaved,
}: {
  entry?: Entry | null;
  onClose: () => void;
  onSaved?: () => void;
}) {
  const data = useVaultStore((state) => state.data);
  const busy = useVaultStore((state) => state.busy);
  const addEntry = useVaultStore((state) => state.addEntry);
  const updateEntry = useVaultStore((state) => state.updateEntry);
  const toast = useToast();

  const initialDraft = useMemo<EntryDraft>(
    () => (entry ? entryToDraft(entry) : { ...emptyEntryDraft }),
    [entry],
  );
  const [draft, setDraft] = useState<EntryDraft>(initialDraft);
  const [nameError, setNameError] = useState<string | null>(null);
  const [blockedError, setBlockedError] = useState<string | null>(null);
  const [strength, setStrength] = useState<PasswordStrength | null>(null);
  const [saving, setSaving] = useState(false);

  const duplicates = useMemo(
    () => findDuplicatePasswords(data?.entries ?? [], draft.password, entry?.id),
    [data?.entries, draft.password, entry?.id],
  );

  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      if (!draft.password) {
        if (!cancelled) setStrength(null);
        return;
      }
      void passwordStrength(draft.password).then((result) => {
        if (!cancelled) setStrength(result);
      });
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [draft.password]);

  const update = <K extends keyof EntryDraft>(key: K, value: EntryDraft[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    if (key === "name") setNameError(null);
    setBlockedError(null);
  };

  const dirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(initialDraft),
    [draft, initialDraft],
  );

  const requestClose = () => {
    if (dirty && !window.confirm("Не сохранять изменения в записи?")) return;
    onClose();
  };

  const save = async () => {
    if (!draft.name.trim()) {
      setNameError("Введите название записи");
      return;
    }

    setSaving(true);
    setBlockedError(null);
    try {
      if (entry) {
        const result = await updateEntry(entry.id, draft);
        if (result.blockedReason === "old-password") {
          setBlockedError("Этот пароль уже использовался в этой записи. Выберите другой.");
          return;
        }
        if (result.ok) {
          toast.show({ title: "Запись обновлена", tone: "success" });
          onSaved?.();
          onClose();
        }
      } else {
        const ok = await addEntry(draft);
        if (ok) {
          toast.show({ title: "Запись создана", tone: "success" });
          onSaved?.();
          onClose();
        }
      }
    } finally {
      setSaving(false);
    }
  };

  const weak = strength !== null && strength.score <= 2;

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) requestClose();
      }}
      title={entry ? "Редактировать запись" : "Новая запись"}
      description={entry ? entry.name : "Название обязательно, остальное можно добавить позже."}
      className="w-[min(92vw,680px)]"
      footer={
        <>
          <Button variant="ghost" onClick={requestClose} disabled={saving}>
            Отменить
          </Button>
          <Button onClick={() => void save()} disabled={saving || busy}>
            {saving ? <Loader2 size={15} className="animate-spin" /> : <Save size={15} />}
            Сохранить
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <Field label="Название" htmlFor="entry-name" error={nameError ?? undefined}>
          <Input
            id="entry-name"
            value={draft.name}
            onChange={(event) => update("name", event.target.value)}
            placeholder="Например, GitHub"
            autoFocus
          />
        </Field>

        <Field label="URL" htmlFor="entry-url" hint="Если указать, рядом с названием появится иконка сайта.">
          <Input
            id="entry-url"
            value={draft.url}
            onChange={(event) => update("url", event.target.value)}
            placeholder="https://example.com"
            inputMode="url"
          />
        </Field>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Логин" htmlFor="entry-login">
            <Input
              id="entry-login"
              value={draft.login}
              onChange={(event) => update("login", event.target.value)}
              placeholder="Можно оставить пустым"
              autoComplete="off"
            />
          </Field>
          <Field label="Пароль" htmlFor="entry-password">
            <SecretInput
              id="entry-password"
              value={draft.password}
              onChange={(event) => update("password", event.target.value)}
              placeholder="Можно оставить пустым"
            />
          </Field>
        </div>

        {draft.password ? (
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <div className="flex gap-1">
                {[0, 1, 2, 3].map((index) => (
                  <span
                    key={index}
                    className={cn(
                      "h-1 w-7 rounded-full",
                      strength && index < strength.score
                        ? strength.score <= 1
                          ? "bg-[var(--danger)]"
                          : strength.score === 2
                            ? "bg-[var(--warning)]"
                            : "bg-[var(--accent)]"
                        : "bg-[var(--surface-3)]",
                    )}
                  />
                ))}
              </div>
              <span
                className={cn(
                  "text-[10.5px]",
                  weak ? "text-[var(--warning)]" : "text-[var(--muted)]",
                )}
              >
                {strength?.label ?? "Проверяем…"}
              </span>
            </div>
            {weak ? (
              <span className="flex items-center gap-1 text-[10.5px] text-[var(--warning)]">
                <AlertTriangle size={12} />
                Слабый пароль — сохранить всё равно можно
              </span>
            ) : strength && strength.score >= 3 ? (
              <span className="flex items-center gap-1 text-[10.5px] text-[var(--accent-soft)]">
                <ShieldCheck size={12} />
                Хороший пароль
              </span>
            ) : null}
          </div>
        ) : null}

        {duplicates.length > 0 ? (
          <div className="flex items-start gap-2 rounded-[var(--radius-sm)] border border-[var(--warning)]/30 bg-[var(--warning)]/8 px-3 py-2.5">
            <AlertTriangle size={14} className="mt-0.5 shrink-0 text-[var(--warning)]" />
            <p className="text-[11px] leading-relaxed text-[var(--text-soft)]">
              Этот пароль уже используется в:{" "}
              <span className="font-medium text-[var(--warning)]">
                {duplicates.map((item) => item.name).join(", ")}
              </span>
              . Можно сохранить, если так задумано.
            </p>
          </div>
        ) : null}

        {blockedError ? (
          <p
            role="alert"
            className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-[11.5px] text-[var(--danger)]"
          >
            {blockedError}
          </p>
        ) : null}

        <Field label="Комментарий" htmlFor="entry-comment">
          <Textarea
            id="entry-comment"
            value={draft.comment}
            onChange={(event) => update("comment", event.target.value)}
            placeholder="Заметка, которую увидите в карточке"
            rows={3}
          />
        </Field>
      </div>
    </Modal>
  );
}
