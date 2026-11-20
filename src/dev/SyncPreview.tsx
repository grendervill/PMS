import { useState } from "react";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { ServerSection } from "@/features/settings/ServerSection";
import { ConflictDialog } from "@/features/sync/ConflictDialog";
import type { SyncStatus } from "@/features/sync/controller";

/**
 * Витрина для визуальной проверки серверных экранов без настоящего сервера.
 * Собирается только в dev-сборке (`import.meta.env.DEV`).
 */
export function SyncPreview() {
  const [conflict, setConflict] = useState(false);
  const statuses: SyncStatus[] = [
    { kind: "idle" },
    { kind: "dirty" },
    { kind: "offline", message: "нет сети" },
  ];

  const profile = {
    id: "p-1",
    name: "pms.home",
    baseUrl: "https://pms.home:8443",
    accountId: "ab".repeat(16),
    login: "alice",
    tls: { kind: "pin" as const, spkiSha256: "cd".repeat(32) },
    lastLoginAt: new Date().toISOString(),
  };

  return (
    <ToastProvider>
      <ConfirmProvider>
        <main className="min-h-screen bg-[var(--bg)] px-6 py-10 text-[var(--text)]">
          <div className="mx-auto grid max-w-3xl gap-4">
            {statuses.map((status, index) => (
              <ServerSection
                key={index}
                profile={profile}
                status={status}
                usage={{
                  used: index === 0 ? 268_435_456 : 980_000_000,
                  quota: 1_073_741_824,
                  vaultBytes: 4_000_000,
                  blobsBytes: 264_435_456,
                  blobCount: 3,
                }}
                stale={status.kind === "offline"}
                onSyncNow={() => setConflict(false)}
                onDisconnect={() => undefined}
                onDeleteAccount={() => setConflict(true)}
              />
            ))}
          </div>
          <ConflictDialog
            open={conflict}
            remoteVersion={7}
            onKeepMine={() => setConflict(false)}
            onTakeTheirs={() => setConflict(false)}
            onLater={() => setConflict(false)}
          />
        </main>
      </ConfirmProvider>
    </ToastProvider>
  );
}
