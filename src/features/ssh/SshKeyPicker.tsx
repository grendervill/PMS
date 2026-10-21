import { KeySquare } from "lucide-react";
import { useVaultStore } from "@/features/vault/store";
import type { SshKey } from "@/features/vault/types";
import { cn } from "@/lib/cn";

function typeLabel(key: SshKey) {
  if (key.type === "rsa") return `RSA ${key.rsaBits ?? 2048}`;
  if (key.type === "ecdsa") return `ECDSA ${(key.curve ?? "p256").toUpperCase()}`;
  return "Ed25519";
}

export function SshKeyPicker({
  ids,
  onChange,
}: {
  ids: string[];
  onChange: (ids: string[]) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const keys = data?.sshKeys ?? [];

  const toggle = (id: string) => {
    onChange(ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id]);
  };

  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/50 p-3.5">
      <div>
        <p className="flex items-center gap-1.5 text-[12.5px] font-medium text-[var(--text-soft)]">
          <KeySquare size={13} className="text-[var(--accent-soft)]" />
          SSH-ключи
        </p>
        <p className="mt-1 text-[10.5px] text-[var(--muted)]">
          Привяжите ключи, которые относятся к этой записи.
        </p>
      </div>

      {keys.length > 0 ? (
        <div className="mt-3 grid gap-1.5">
          {keys.map((key) => {
            const checked = ids.includes(key.id);
            return (
              <label
                key={key.id}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 rounded-[var(--radius-sm)] border px-2.5 py-2 transition-colors",
                  checked
                    ? "border-[var(--accent)]/35 bg-[var(--accent)]/5"
                    : "border-[var(--border)] bg-[var(--surface)] hover:border-[var(--border-strong)]",
                )}
              >
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={() => toggle(key.id)}
                  className="size-3.5 shrink-0 accent-[var(--accent)]"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[12px] text-[var(--text-soft)]">
                    {key.name}
                  </span>
                  <span className="block truncate text-[10px] text-[var(--muted)]">
                    {typeLabel(key)} · {key.comment || "без комментария"}
                  </span>
                </span>
              </label>
            );
          })}
        </div>
      ) : (
        <p className="mt-3 rounded-[var(--radius-sm)] border border-dashed border-[var(--border)] px-3 py-2.5 text-[11px] text-[var(--muted)]">
          Ключей пока нет. Создайте их в разделе SSH — потом вернитесь сюда.
        </p>
      )}
    </div>
  );
}
