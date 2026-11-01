import {
  ChevronRight,
  CreditCard,
  IdCard,
  KeyRound,
  KeySquare,
  Star,
  Timer,
} from "lucide-react";
import { EmptyState } from "@/components/ui/EmptyState";
import { cardLabel } from "@/features/cards/cardModel";
import {
  documentChip,
  documentTitle,
  maskDocumentNumber,
} from "@/features/documents/documentModel";
import { entryUrls, hostFromUrl } from "@/features/entries/entries";
import {
  FAVORITE_SECTIONS,
  favoritesOf,
} from "@/features/favorites/favorites";
import { FavoriteButton } from "@/features/favorites/FavoriteButton";
import { TotpCode } from "@/features/totp/TotpCode";
import { useVaultStore } from "@/features/vault/store";
import type {
  FavoriteKind,
  FavoriteRef,
  TotpConfig,
} from "@/features/vault/types";

const groupIcons = {
  entry: KeyRound,
  totp: Timer,
  ssh: KeySquare,
  card: CreditCard,
  document: IdCard,
} satisfies Record<FavoriteKind, typeof KeyRound>;

type FavoriteRow = {
  ref: FavoriteRef;
  title: string;
  subtitle: string;
  totp?: TotpConfig;
  onOpen: () => void;
};

function sshTypeLabel(key: {
  type: "ed25519" | "rsa" | "ecdsa";
  rsaBits?: number | null;
  curve?: string | null;
}) {
  if (key.type === "rsa") return `RSA ${key.rsaBits ?? 2048}`;
  if (key.type === "ecdsa") return `ECDSA ${(key.curve ?? "p256").toUpperCase()}`;
  return "Ed25519";
}

export function FavoritesView({
  onOpenEntry,
  onOpenSshKey,
  onOpenCard,
  onOpenDocument,
}: {
  onOpenEntry: (id: string) => void;
  onOpenSshKey: (id: string) => void;
  onOpenCard: (id: string) => void;
  onOpenDocument: (id: string) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const favorites = favoritesOf(data);
  const entries = data?.entries ?? [];
  const sshKeys = data?.sshKeys ?? [];
  const cards = data?.bankCards ?? [];
  const documents = data?.documents ?? [];

  const rows = (kind: FavoriteKind): FavoriteRow[] =>
    favorites
      .filter((item) => item.kind === kind)
      .flatMap((ref) => {
        if (kind === "entry" || kind === "totp") {
          const entry = entries.find((item) => item.id === ref.id);
          if (!entry) return [];
          if (kind === "totp" && !entry.totp) return [];
          const host = entryUrls(entry)[0];
          return [
            {
              ref,
              title: entry.name,
              subtitle:
                kind === "totp"
                  ? entry.totp?.issuer || entry.login || "TOTP"
                  : entry.login || (host ? hostFromUrl(host) : "") || "Запись",
              totp: kind === "totp" ? (entry.totp ?? undefined) : undefined,
              onOpen: () => onOpenEntry(entry.id),
            },
          ];
        }
        if (kind === "ssh") {
          const key = sshKeys.find((item) => item.id === ref.id);
          if (!key) return [];
          return [
            {
              ref,
              title: key.name,
              subtitle: `${sshTypeLabel(key)}${
                key.fingerprint
                  ? ` · ${key.fingerprint.replace("SHA256:", "")}`
                  : ""
              }`,
              onOpen: () => onOpenSshKey(key.id),
            },
          ];
        }
        if (kind === "card") {
          const card = cards.find((item) => item.id === ref.id);
          if (!card) return [];
          return [
            {
              ref,
              title: cardLabel(card),
              subtitle: card.holder || "Банковская карта",
              onOpen: () => onOpenCard(card.id),
            },
          ];
        }
        const document = documents.find((item) => item.id === ref.id);
        if (!document) return [];
        return [
          {
            ref,
            title: documentTitle(document),
            subtitle: `${documentChip(document)} · ${maskDocumentNumber(document)}`,
            onOpen: () => onOpenDocument(document.id),
          },
        ];
      });

  const groups = FAVORITE_SECTIONS.map((section) => ({
    ...section,
    rows: rows(section.kind),
  })).filter((group) => group.rows.length > 0);

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-3xl">
        {groups.length === 0 ? (
          <EmptyState
            icon={Star}
            title="Пока нет избранного"
            description="Отметьте звездой запись, TOTP, SSH-ключ, карту или документ — они появятся здесь."
          />
        ) : (
          groups.map((group) => {
            const Icon = groupIcons[group.kind];
            return (
              <section key={group.kind} className="mb-5 last:mb-0">
                <div className="mb-2 flex items-center gap-2 px-0.5">
                  <Icon size={14} className="text-[var(--accent-soft)]" />
                  <h2 className="text-[12px] font-medium text-[var(--text-soft)]">
                    {group.title}
                  </h2>
                  <span className="text-[10.5px] text-[var(--muted)]/70">
                    {group.rows.length}
                  </span>
                </div>
                <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/50">
                  {group.rows.map((row) => (
                    <div
                      key={`${group.kind}-${row.ref.id}`}
                      className="flex items-center gap-1.5 border-b border-[var(--border)] px-2.5 py-2 last:border-b-0"
                    >
                      <button
                        type="button"
                        onClick={row.onOpen}
                        className="flex min-w-0 flex-1 items-center gap-3 rounded-[var(--radius-sm)] px-1.5 py-1 text-left transition-colors hover:bg-[var(--surface-2)]/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
                      >
                        <span className="grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                          <Icon size={15} />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[12.5px] font-medium text-[var(--text)]">
                            {row.title}
                          </span>
                          <span className="mt-0.5 block truncate text-[10.5px] text-[var(--muted)]">
                            {row.subtitle}
                          </span>
                        </span>
                      </button>
                      {row.totp ? <TotpCode config={row.totp} compact /> : null}
                      <FavoriteButton
                        kind={group.kind}
                        id={row.ref.id}
                        label={
                          group.kind === "totp"
                            ? "Убрать TOTP из избранного"
                            : "Убрать из избранного"
                        }
                        activeLabel={
                          group.kind === "totp"
                            ? "Убрать TOTP из избранного"
                            : "Убрать из избранного"
                        }
                      />
                      <ChevronRight
                        size={14}
                        className="shrink-0 text-[var(--muted)]"
                      />
                    </div>
                  ))}
                </div>
              </section>
            );
          })
        )}
      </div>
    </div>
  );
}
