import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type Entry, type SshKey, type VaultData } from "@/features/vault/types";
import { EntryDetail } from "./EntryDetail";

const entry: Entry = {
  id: "entry-1",
  kind: "login",
  schemaVersion: 1,
  name: "GitHub",
  login: "alex",
  password: "s3cret",
  url: "https://github.com",
  urls: ["https://github.com", "https://gist.github.com"],
  comment: "",
  totp: null,
  attachmentIds: [],
  sshKeyIds: [],
  loginHistory: [],
  passwordHistory: [],
  createdAt: "2026-10-20T10:00:00.000Z",
  updatedAt: "2026-10-20T10:00:00.000Z",
};

function renderDetail(
  currentEntry: Entry = entry,
  onOpenSshKey: (id: string) => void = vi.fn(),
) {
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <EntryDetail
          entry={currentEntry}
          onBack={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          onHistory={vi.fn()}
          onOpenSshKey={onOpenSshKey}
        />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

describe("EntryDetail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reveals a password and hides it again after 30 seconds", () => {
    renderDetail();

    fireEvent.click(screen.getByRole("button", { name: "Показать пароль" }));
    expect(screen.getByText("s3cret")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(30_000);
    });

    expect(screen.queryByText("s3cret")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Показать пароль" })).toBeInTheDocument();
  });

  it("shows every url in the entry", () => {
    renderDetail();

    expect(screen.getByText("https://github.com")).toBeInTheDocument();
    expect(screen.getByText("https://gist.github.com")).toBeInTheDocument();
  });
});

  it("opens a linked SSH key from its name and menu", () => {
    const sshKey: SshKey = {
      id: "ssh-1",
      name: "GitHub key",
      type: "ed25519",
      publicKey: "ssh-ed25519 AAAA test",
      privateKey: "private",
      comment: "",
      attachmentIds: [],
      createdAt: "2026-10-20T10:00:00.000Z",
      updatedAt: "2026-10-20T10:00:00.000Z",
    };
    const linkedEntry: Entry = { ...entry, sshKeyIds: [sshKey.id] };
    const vaultData: VaultData = {
      schemaVersion: 1,
      id: "vault-1",
      name: "Личное",
      createdAt: entry.createdAt,
      updatedAt: entry.updatedAt,
      settings: defaultVaultSettings(),
      entries: [linkedEntry],
      sshKeys: [sshKey],
      attachments: [],
      drafts: [],
    };
    useVaultStore.setState({ ...initialVaultState, status: "unlocked", data: vaultData });
    const onOpenSshKey = vi.fn();
    renderDetail(linkedEntry, onOpenSshKey);

    fireEvent.click(screen.getByRole("button", { name: "GitHub key" }));
    expect(onOpenSshKey).toHaveBeenCalledWith("ssh-1");

    fireEvent.click(screen.getByRole("button", { name: "Действия с ключом GitHub key" }));
    fireEvent.click(screen.getByRole("button", { name: "Открыть ключ в SSH" }));
    expect(onOpenSshKey).toHaveBeenCalledTimes(2);
  });
