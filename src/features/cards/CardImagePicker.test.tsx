import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage, type VaultStorage } from "@/lib/storage";
import { vaultApi } from "@/features/vault/api";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import {
  defaultVaultSettings,
  type AttachmentMeta,
  type VaultData,
} from "@/features/vault/types";
import { CardImagePicker } from "./CardImagePicker";

vi.mock("@/lib/storage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/storage")>();
  return { ...actual, getStorage: vi.fn() };
});

const meta: AttachmentMeta = {
  id: "att-1",
  name: "card.png",
  mime: "image/png",
  size: 3,
  sha256: "hash",
  kind: "image",
  createdAt: "2026-10-24T10:00:00.000Z",
};

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: meta.createdAt,
  updatedAt: meta.createdAt,
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  bankCards: [],
  documents: [],
  attachments: [],
  drafts: [],
};

function storage(): VaultStorage {
  return {
    pickFolder: vi.fn(),
    pickFile: vi.fn().mockResolvedValue("/tmp/card.png"),
    pickSaveFile: vi.fn(),
    displayName: vi.fn(),
    readFile: vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3])),
    vaultExists: vi.fn(),
    ensureLayout: vi.fn(),
    readVault: vi.fn(),
    writeVault: vi.fn(),
    writeBackup: vi.fn(),
    listBackups: vi.fn(),
    deleteBackup: vi.fn(),
    statVault: vi.fn(),
    exists: vi.fn(),
    mkdir: vi.fn(),
    saveFile: vi.fn(),
    readBlob: vi.fn(),
    writeBlob: vi.fn(),
    listBlobIds: vi.fn(),
    deleteBlob: vi.fn(),
  };
}

describe("CardImagePicker", () => {
  const onPreset = vi.fn();
  const onAttachment = vi.fn();
  const onError = vi.fn();
  const addAttachmentMeta = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    vi.mocked(getStorage).mockReturnValue(storage());
    vi.spyOn(vaultApi, "sealAttachment").mockResolvedValue({
      meta,
      blob: new Uint8Array([1, 2, 3]),
    });
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
      addAttachmentMeta,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  function renderPicker(attachmentId: string | null = null) {
    return render(
      <CardImagePicker
        preset="emerald"
        attachmentId={attachmentId}
        imageUrl={null}
        onPreset={onPreset}
        onAttachment={onAttachment}
        onError={onError}
      />,
    );
  }

  it("selects a preset", () => {
    renderPicker();
    fireEvent.click(screen.getByRole("button", { name: "Полночь" }));
    expect(onPreset).toHaveBeenCalledWith("midnight");
  });

  it("uploads an image and stores it as an encrypted attachment", async () => {
    renderPicker();
    fireEvent.click(screen.getByRole("button", { name: /Загрузить картинку/ }));

    await waitFor(() => {
      expect(onAttachment).toHaveBeenCalledWith("att-1");
    });
    expect(vaultApi.sealAttachment).toHaveBeenCalledWith(
      "session-1",
      "card.png",
      "image/png",
      new Uint8Array([1, 2, 3]),
    );
    expect(getStorage().writeBlob).toHaveBeenCalledWith(
      "/vault",
      "att-1",
      new Uint8Array([1, 2, 3]),
    );
    expect(addAttachmentMeta).toHaveBeenCalledWith(meta);
  });

  it("rejects a non-image file", async () => {
    vi.mocked(getStorage().pickFile).mockResolvedValue("/tmp/notes.txt");
    renderPicker();
    fireEvent.click(screen.getByRole("button", { name: /Загрузить картинку/ }));

    await waitFor(() => {
      expect(onError).toHaveBeenCalledWith(
        "Выберите картинку в формате PNG, JPEG или WebP",
      );
    });
    expect(vaultApi.sealAttachment).not.toHaveBeenCalled();
    expect(onAttachment).not.toHaveBeenCalled();
  });

  it("removes a custom image", () => {
    render(
      <CardImagePicker
        preset="emerald"
        attachmentId="att-1"
        imageUrl="blob:card"
        onPreset={onPreset}
        onAttachment={onAttachment}
        onError={onError}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Убрать картинку" }));
    expect(onAttachment).toHaveBeenCalledWith(null);
  });
});
