import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { createBankCard, emptyCardDraft } from "@/features/cards/cardModel";
import { createDocument, emptyDocumentDraft } from "@/features/documents/documentModel";
import { createEntry } from "@/features/entries/entries";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import {
  defaultVaultSettings,
  type SshKey,
  type VaultData,
} from "@/features/vault/types";
import { FavoritesView } from "./FavoritesView";

const entry = createEntry(
  {
    name: "GitHub",
    login: "alex",
    password: "s3cret",
    urls: ["https://github.com"],
    comment: "",
    attachmentIds: [],
    sshKeyIds: [],
    totp: null,
    totpRaw: "",
  },
  "2026-10-24T10:00:00.000Z",
);

const totpEntry = createEntry(
  {
    name: "GitHub 2FA",
    login: "alex",
    password: "s3cret",
    urls: [],
    comment: "",
    attachmentIds: [],
    sshKeyIds: [],
    totp: {
      secret: "JBSWY3DPEHPK3PXP",
      algorithm: "SHA1",
      digits: 6,
      period: 30,
    },
    totpRaw: "JBSWY3DPEHPK3PXP",
  },
  "2026-10-24T10:00:00.000Z",
);

const sshKey: SshKey = {
  id: "ssh-1",
  name: "GitHub key",
  type: "ed25519",
  publicKey: "ssh-ed25519 AAAA test",
  privateKey: "private",
  comment: "",
  attachmentIds: [],
  createdAt: "2026-10-24T10:00:00.000Z",
  updatedAt: "2026-10-24T10:00:00.000Z",
};

const card = createBankCard(
  {
    ...emptyCardDraft(),
    number: "4111111111111111",
    expiry: "12/29",
    cvv: "123",
    holder: "IVAN IVANOV",
    purpose: "Основная",
  },
  "2026-10-24T10:00:00.000Z",
);

const document = createDocument(
  {
    ...emptyDocumentDraft("passport"),
    passportKind: "main",
    fullName: "ИВАНОВ ИВАН",
    series: "4509",
    number: "123456",
  },
  "2026-10-24T10:00:00.000Z",
);

function vault(favorites: VaultData["favorites"]): VaultData {
  return {
    schemaVersion: 1,
    id: "vault-1",
    name: "Личное",
    createdAt: "2026-10-24T10:00:00.000Z",
    updatedAt: "2026-10-24T10:00:00.000Z",
    settings: defaultVaultSettings(),
    entries: [entry, totpEntry],
    sshKeys: [sshKey],
    bankCards: [card],
    documents: [document],
    attachments: [],
    drafts: [],
    favorites,
  };
}

const allFavorites: VaultData["favorites"] = [
  { kind: "entry", id: entry.id },
  { kind: "totp", id: totpEntry.id },
  { kind: "ssh", id: sshKey.id },
  { kind: "card", id: card.id },
  { kind: "document", id: document.id },
];

function renderView() {
  const onOpenEntry = vi.fn();
  const onOpenSshKey = vi.fn();
  const onOpenCard = vi.fn();
  const onOpenDocument = vi.fn();
  render(
    <ToastProvider>
      <FavoritesView
        onOpenEntry={onOpenEntry}
        onOpenSshKey={onOpenSshKey}
        onOpenCard={onOpenCard}
        onOpenDocument={onOpenDocument}
      />
    </ToastProvider>,
  );
  return { onOpenEntry, onOpenSshKey, onOpenCard, onOpenDocument };
}

describe("FavoritesView", () => {
  beforeEach(() => {
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      data: vault(allFavorites),
    });
  });

  it("groups favorites and opens each kind", () => {
    const callbacks = renderView();

    for (const title of ["Записи", "TOTP", "SSH-ключи", "Карты", "Документы"]) {
      expect(screen.getByRole("heading", { name: title })).toBeInTheDocument();
    }

    fireEvent.click(screen.getByText("GitHub", { exact: true }));
    expect(callbacks.onOpenEntry).toHaveBeenCalledWith(entry.id);

    fireEvent.click(screen.getByText("GitHub key"));
    expect(callbacks.onOpenSshKey).toHaveBeenCalledWith(sshKey.id);

    fireEvent.click(screen.getByText("Основная · 1111"));
    expect(callbacks.onOpenCard).toHaveBeenCalledWith(card.id);

    fireEvent.click(screen.getByText("Паспорт РФ"));
    expect(callbacks.onOpenDocument).toHaveBeenCalledWith(document.id);
  });

  it("shows an empty state when nothing is favorited", () => {
    useVaultStore.setState({ data: vault([]) });
    renderView();

    expect(screen.getByText("Пока нет избранного")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Записи" })).not.toBeInTheDocument();
  });
});
