import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage, type VaultStorage } from "@/lib/storage";
import { vaultApi } from "@/features/vault/api";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import {
  defaultVaultSettings,
  type AttachmentMeta,
  type VaultData,
} from "@/features/vault/types";
import { useAttachmentUrl } from "./useAttachmentUrl";

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
  attachments: [meta],
  drafts: [],
};

function storage(): VaultStorage {
  return {
    pickFolder: vi.fn(),
    pickFile: vi.fn(),
    pickSaveFile: vi.fn(),
    displayName: vi.fn(),
    readFile: vi.fn(),
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
    readBlob: vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3])),
    writeBlob: vi.fn(),
    listBlobIds: vi.fn(),
    deleteBlob: vi.fn(),
  };
}

function Probe({ id }: { id: string | null }) {
  const url = useAttachmentUrl(id);
  return <span data-testid="url">{url ?? "none"}</span>;
}

describe("useAttachmentUrl", () => {
  beforeEach(() => {
    vi.mocked(getStorage).mockReturnValue(storage());
    vi.spyOn(vaultApi, "openAttachment").mockResolvedValue(new Uint8Array([1, 2, 3]));
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns an object URL for a stored image", async () => {
    const view = render(<Probe id="att-1" />);

    await waitFor(() => {
      expect(screen.getByTestId("url")).toHaveTextContent("blob:test");
    });
    expect(getStorage().readBlob).toHaveBeenCalledWith("/vault", "att-1");
    expect(vaultApi.openAttachment).toHaveBeenCalledWith(
      "session-1",
      "att-1",
      new Uint8Array([1, 2, 3]),
    );

    view.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:test");
  });

  it("returns null without an id", () => {
    render(<Probe id={null} />);
    expect(screen.getByTestId("url")).toHaveTextContent("none");
    expect(getStorage().readBlob).not.toHaveBeenCalled();
  });
});
