import { KeySquare, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { SshCreateDialog } from "@/features/ssh/SshCreateDialog";
import { SshDetail } from "@/features/ssh/SshDetail";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { SshKey } from "@/features/vault/types";
import { cn } from "@/lib/cn";

function typeLabel(key: SshKey) {
  if (key.type === "rsa") return `RSA ${key.rsaBits ?? 2048}`;
  if (key.type === "ecdsa") return `ECDSA ${(key.curve ?? "p256").toUpperCase()}`;
  return "Ed25519";
}

export function SSHView({ onOpenEntry }: { onOpenEntry?: (id: string) => void }) {
  const data = useVaultStore((state) => state.data);
  const deleteSshKey = useVaultStore((state) => state.deleteSshKey);
  const confirm = useConfirm();
  const toast = useToast();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [createOpen, setCreateOpen] = useState(false);

  const keys = useMemo(() => data?.sshKeys ?? [], [data?.sshKeys]);
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return keys;
    return keys.filter((key) =>
      `${key.name} ${key.comment} ${key.publicKey}`.toLowerCase().includes(needle),
    );
  }, [keys, query]);
  const selected = keys.find((key) => key.id === selectedId) ?? null;
  const linkedCount = (key: SshKey) =>
    (data?.entries ?? []).filter((entry) => (entry.sshKeyIds ?? []).includes(key.id)).length;

  const remove = async (key: SshKey) => {
    const allowed = await confirm({
      title: `Удалить ключ «${key.name}»?`,
      description: "Установленные на диск файлы останутся, но ключ исчезнет из хранилища и записей.",
      confirmLabel: "Удалить",
      tone: "danger",
    });
    if (!allowed) return;

    const ok = await deleteSshKey(key.id);
    if (ok) {
      toast.show({ title: "SSH-ключ удалён", tone: "success" });
      if (selectedId === key.id) setSelectedId(null);
    } else {
      toast.show({
        title: "Не удалось удалить ключ",
        description: errorText(useVaultStore.getState().error),
        tone: "error",
      });
    }
  };

  const onCreated = (id: string) => {
    setSelectedId(id);
    setCreateOpen(false);
  };

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <section
        className={cn(
          "min-h-0 w-full flex-col border-r border-[var(--border)] bg-[var(--surface)]/35 lg:flex lg:w-[380px] lg:shrink-0",
          selected ? "hidden" : "flex",
        )}
      >
        <div className="flex h-full min-h-0 flex-col">
          <div className="border-b border-[var(--border)] px-4 py-3.5">
            <div className="flex items-center gap-2">
              <div className="relative min-w-0 flex-1">
                <Search
                  size={14}
                  className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--muted)]"
                />
                <Input
                  aria-label="Поиск по SSH-ключам"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Поиск по ключам"
                  className="h-8 pl-8 text-[12px]"
                />
              </div>
              <Button size="sm" onClick={() => setCreateOpen(true)}>
                <Plus size={14} />
                Ключ
              </Button>
            </div>
            <div className="mt-2.5 flex items-center justify-between">
              <span className="text-[10.5px] text-[var(--muted)]">
                {keys.length === 0 ? "Нет ключей" : `${keys.length} ${keys.length === 1 ? "ключ" : "ключей"}`}
              </span>
              <span className="text-[10px] text-[var(--muted)]/70">OpenSSH в vault</span>
            </div>
          </div>

          <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto">
            {filtered.length > 0 ? (
              <div className="divide-y divide-[var(--border)]/70">
                {filtered.map((key) => {
                  const count = linkedCount(key);
                  return (
                    <button
                      key={key.id}
                      type="button"
                      onClick={() => setSelectedId(key.id)}
                      className={cn(
                        "group flex w-full items-start gap-3 px-4 py-3 text-left transition-colors",
                        key.id === selectedId
                          ? "bg-[var(--surface-2)]"
                          : "hover:bg-[var(--surface-2)]/60",
                      )}
                    >
                      <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-[9px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                        <KeySquare size={16} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-2">
                          <span className="truncate text-[13px] font-medium text-[var(--text)]">
                            {key.name}
                          </span>
                          {count > 0 ? <Badge tone="accent">{count}</Badge> : null}
                        </span>
                        <span className="mt-0.5 block truncate text-[10.5px] text-[var(--muted)]">
                          {typeLabel(key)} ·{" "}
                          {key.fingerprint?.replace("SHA256:", "") ?? key.publicKey.slice(0, 28)}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="p-4">
                <EmptyState
                  icon={Search}
                  title={query ? "Ничего не найдено" : "Пока нет SSH-ключей"}
                  description={
                    query
                      ? "Попробуйте другое слово или очистите поиск."
                      : "Создайте Ed25519, RSA или ECDSA ключ. Он будет храниться в зашифрованном виде."
                  }
                  action={
                    query ? (
                      <Button variant="secondary" size="sm" onClick={() => setQuery("")}>
                        Очистить поиск
                      </Button>
                    ) : (
                      <Button size="sm" onClick={() => setCreateOpen(true)}>
                        <Plus size={14} />
                        Создать ключ
                      </Button>
                    )
                  }
                  className="min-h-[240px]"
                />
              </div>
            )}
          </div>
        </div>
      </section>

      <section
        className={cn(
          "min-h-0 min-w-0 flex-1 flex-col",
          selected ? "flex" : "hidden lg:flex",
        )}
      >
        {selected ? (
          <SshDetail
            key={selected.id}
            sshKey={selected}
            onBack={() => setSelectedId(null)}
            onDelete={() => void remove(selected)}
            onOpenEntry={onOpenEntry}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center p-6">
            <EmptyState
              icon={KeySquare}
              title="Выберите SSH-ключ"
              description="Публичный ключ, fingerprint, форматы и установка на устройство."
              className="w-full max-w-xl"
            />
          </div>
        )}
      </section>

      {createOpen ? (
        <SshCreateDialog
          onClose={() => setCreateOpen(false)}
          onCreated={onCreated}
        />
      ) : null}
    </div>
  );
}
