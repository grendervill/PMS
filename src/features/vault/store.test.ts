import { beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage } from "@/lib/storage";
import { addRecent } from "@/lib/storage/recent";
import type { RecentVault, VaultStorage } from "@/lib/storage";
import { createEntry } from "@/features/entries/entries";
import { isBiometricEnabled } from "@/features/vault/biometric";
import { VaultApiError, vaultApi } from "@/features/vault/api";
import { initialVaultState, useVaultStore } from "./store";
import { defaultVaultSettings, type VaultData, type VaultPublicInfo } from "./types";

vi.mock("@/lib/storage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/storage")>();
  return { ...actual, getStorage: vi.fn() };
});

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    vaultApi: {
      inspect: vi.fn(),
      create: vi.fn(),
      unlock: vi.fn(),
      save: vi.fn(),
      rewrap: vi.fn(),
      lock: vi.fn(),
      biometricStatus: vi.fn(),
      biometricEnroll: vi.fn(),
      biometricDisable: vi.fn(),
      biometricUnlock: vi.fn(),
    },
  };
});

const createdAt = "2026-10-20T10:00:00.000Z";

const vaultData: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt,
  updatedAt: createdAt,
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  attachments: [],
  drafts: [],
};

const publicInfo: VaultPublicInfo = {
  vaultId: "vault-1",
  format: 1,
  createdAt,
  cipher: "xchacha20-poly1305",
  slotKinds: ["passphrase", "keyfile"],
  fileHash: "hash",
};

function makeStorage(): VaultStorage {
  return {
    pickFolder: vi.fn(),
    pickFile: vi.fn(),
    pickSaveFile: vi.fn(),
    displayName: vi.fn((value: string) => value.split("/").at(-1) ?? "Хранилище"),
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
    readBlob: vi.fn(),
    writeBlob: vi.fn(),
    listBlobIds: vi.fn().mockResolvedValue([]),
    deleteBlob: vi.fn(),
  };
}

let storage: VaultStorage;

describe("vault store", () => {
  beforeEach(() => {
    localStorage.clear();
    useVaultStore.setState(initialVaultState);
    vi.resetAllMocks();
    storage = makeStorage();
    vi.mocked(getStorage).mockReturnValue(storage);
  });

  it("stays on welcome when there are no recent vaults", async () => {
    await useVaultStore.getState().init();

    expect(useVaultStore.getState().status).toBe("welcome");
    expect(useVaultStore.getState().initialized).toBe(true);
  });

  it("opens a recent vault and waits for unlock", async () => {
    const recent: RecentVault = {
      path: "/vaults/personal",
      name: "Личное",
      vaultId: "vault-1",
      lastOpenedAt: createdAt,
    };
    addRecent(recent);
    vi.mocked(storage.readVault).mockResolvedValue(new Uint8Array([1, 2]));
    vi.mocked(vaultApi.inspect).mockResolvedValue(publicInfo);

    await useVaultStore.getState().init();

    expect(useVaultStore.getState().status).toBe("locked");
    expect(useVaultStore.getState().vaultPath).toBe("/vaults/personal");
    expect(useVaultStore.getState().publicInfo).toEqual(publicInfo);
    expect(useVaultStore.getState().data).toBeNull();
  });

  it("creates a vault, writes the file and unlocks", async () => {
    vi.mocked(storage.vaultExists).mockResolvedValue(false);
    vi.mocked(vaultApi.create).mockResolvedValue({
      sessionId: "session-1",
      data: vaultData,
      file: new Uint8Array([7]),
    });
    vi.mocked(vaultApi.inspect).mockResolvedValue(publicInfo);

    const created = await useVaultStore.getState().createVault({
      folder: "/vaults/personal",
      name: "Личное",
      kdfProfile: "standard",
      cipher: "xchacha20-poly1305",
      primarySecret: { kind: "passphrase", value: "correct horse" },
    });

    expect(created).toBe(true);
    expect(storage.ensureLayout).toHaveBeenCalledWith("/vaults/personal");
    expect(storage.writeVault).toHaveBeenCalledWith("/vaults/personal", new Uint8Array([7]));
    expect(useVaultStore.getState().status).toBe("unlocked");
    expect(useVaultStore.getState().sessionId).toBe("session-1");
    expect(useVaultStore.getState().data).toEqual(vaultData);
  });

  it("adds a secondary keyfile slot before writing", async () => {
    vi.mocked(storage.vaultExists).mockResolvedValue(false);
    vi.mocked(vaultApi.create).mockResolvedValue({
      sessionId: "session-1",
      data: vaultData,
      file: new Uint8Array([1]),
    });
    vi.mocked(vaultApi.rewrap).mockResolvedValue(new Uint8Array([2]));
    vi.mocked(vaultApi.inspect).mockResolvedValue(publicInfo);

    await useVaultStore.getState().createVault({
      folder: "/vaults/personal",
      name: "Личное",
      kdfProfile: "standard",
      cipher: "xchacha20-poly1305",
      primarySecret: { kind: "passphrase", value: "correct horse" },
      secondarySecret: { kind: "keyfile", bytesB64: "a2V5" },
    });

    expect(vaultApi.rewrap).toHaveBeenCalledWith(
      "session-1",
      { kind: "keyfile", bytesB64: "a2V5" },
      true,
    );
    expect(storage.writeVault).toHaveBeenCalledWith("/vaults/personal", new Uint8Array([2]));
  });

  it("refuses to overwrite an existing vault", async () => {
    vi.mocked(storage.vaultExists).mockResolvedValue(true);

    const created = await useVaultStore.getState().createVault({
      folder: "/vaults/personal",
      name: "Личное",
      kdfProfile: "standard",
      cipher: "xchacha20-poly1305",
      primarySecret: { kind: "passphrase", value: "correct horse" },
    });

    expect(created).toBe(false);
    expect(useVaultStore.getState().error?.code).toBe("vault_exists");
    expect(storage.writeVault).not.toHaveBeenCalled();
  });

  it("keeps the vault locked after a wrong secret", async () => {
    useVaultStore.setState({
      status: "locked",
      vaultPath: "/vaults/personal",
      publicInfo,
    });
    vi.mocked(storage.readVault).mockResolvedValue(new Uint8Array([1]));
    vi.mocked(vaultApi.unlock).mockRejectedValue(
      new VaultApiError("invalid_credentials", "Неверный пароль"),
    );

    const unlocked = await useVaultStore
      .getState()
      .unlock({ kind: "passphrase", value: "wrong" });

    expect(unlocked).toBe(false);
    expect(useVaultStore.getState().status).toBe("locked");
    expect(useVaultStore.getState().error?.code).toBe("invalid_credentials");
    expect(useVaultStore.getState().data).toBeNull();
  });

  it("locks and clears decrypted data", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    vi.mocked(vaultApi.lock).mockResolvedValue(undefined);

    await useVaultStore.getState().lock();

    expect(vaultApi.lock).toHaveBeenCalledWith("session-1");
    expect(useVaultStore.getState().status).toBe("locked");
    expect(useVaultStore.getState().sessionId).toBeNull();
    expect(useVaultStore.getState().data).toBeNull();
  });

  it("saves updated data through the API and storage", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    const updated = { ...vaultData, name: "Обновлено" };
    vi.mocked(vaultApi.save).mockResolvedValue(new Uint8Array([9]));

    const saved = await useVaultStore.getState().save(updated);

    expect(saved).toBe(true);
    expect(storage.writeVault).toHaveBeenCalledWith("/vaults/personal", new Uint8Array([9]));
    expect(useVaultStore.getState().data).toEqual(updated);
  });

  it("resets back to welcome", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    vi.mocked(vaultApi.lock).mockResolvedValue(undefined);

    await useVaultStore.getState().reset();

    expect(useVaultStore.getState().status).toBe("welcome");
    expect(useVaultStore.getState().vaultPath).toBeNull();
    expect(useVaultStore.getState().data).toBeNull();
  });

  it("does not forget a recent vault when opening fails", async () => {
    addRecent({
      path: "/vaults/missing",
      name: "Пропавшее",
      vaultId: "vault-1",
      lastOpenedAt: createdAt,
    });
    vi.mocked(storage.readVault).mockRejectedValue(
      new VaultApiError("storage", "Не удалось прочитать папку"),
    );

    const opened = await useVaultStore.getState().openVault("/vaults/missing");

    expect(opened).toBe(false);
    expect(useVaultStore.getState().recent).toHaveLength(1);
    expect(useVaultStore.getState().error?.code).toBe("storage");
  });

  it("adds an entry through save", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    vi.mocked(vaultApi.save).mockResolvedValue(new Uint8Array([1]));

    const saved = await useVaultStore.getState().addEntry({
      name: "GitHub",
      login: "alex",
      password: "s3cret",
      urls: [],
      comment: "",
      attachmentIds: [],
      sshKeyIds: [],
      totp: null,
      totpRaw: "",
    });

    expect(saved).toBe(true);
    expect(useVaultStore.getState().data?.entries[0]?.name).toBe("GitHub");
    expect(storage.writeVault).toHaveBeenCalled();
  });

  it("blocks a historical password through the store", async () => {
    const entry = {
      ...createEntry(
        { name: "GitHub", login: "alex", password: "current", urls: [], comment: "", attachmentIds: [], sshKeyIds: [], totp: null, totpRaw: "" },
        createdAt,
      ),
      passwordHistory: [{ value: "old", changedAt: createdAt }],
    };
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: { ...vaultData, entries: [entry] },
    });

    const result = await useVaultStore.getState().updateEntry(entry.id, {
      name: "GitHub",
      login: "alex",
      password: "old",
      urls: [],
      comment: "",
      attachmentIds: [],
      sshKeyIds: [],
      totp: null,
      totpRaw: "",
    });

    expect(result.blockedReason).toBe("old-password");
    expect(vaultApi.save).not.toHaveBeenCalled();
  });

  it("deletes an entry through save", async () => {
    const entry = createEntry(
      { name: "GitHub", login: "", password: "", urls: [], comment: "", attachmentIds: [], sshKeyIds: [], totp: null, totpRaw: "" },
      createdAt,
    );
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: { ...vaultData, entries: [entry] },
    });
    vi.mocked(vaultApi.save).mockResolvedValue(new Uint8Array([1]));

    const deleted = await useVaultStore.getState().deleteEntry(entry.id);

    expect(deleted).toBe(true);
    expect(useVaultStore.getState().data?.entries).toEqual([]);
  });

  it("adds an SSH key through save", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    vi.mocked(vaultApi.save).mockResolvedValue(new Uint8Array([1]));

    const saved = await useVaultStore.getState().addSshKey({
      name: "GitHub key",
      type: "ed25519",
      rsaBits: null,
      curve: null,
      publicKey: "ssh-ed25519 AAAA test",
      privateKey: "private",
      passphrase: null,
      comment: "for GitHub",
      attachmentIds: [],
    });

    expect(saved).toBe(true);
    expect(useVaultStore.getState().data?.sshKeys[0]?.name).toBe("GitHub key");
    expect(storage.writeVault).toHaveBeenCalled();
  });

  it("deletes an SSH key and unlinks it from entries", async () => {
    const key = {
      id: "ssh-1",
      name: "GitHub key",
      type: "ed25519" as const,
      rsaBits: null,
      curve: null,
      publicKey: "ssh-ed25519 AAAA test",
      privateKey: "private",
      passphrase: null,
      comment: "",
      attachmentIds: [],
      createdAt,
      updatedAt: createdAt,
    };
    const entry = {
      ...createEntry(
        {
          name: "GitHub",
          login: "alex",
          password: "",
          urls: [],
          comment: "",
          attachmentIds: [],
          sshKeyIds: ["ssh-1"],
          totp: null,
          totpRaw: "",
        },
        createdAt,
      ),
    };
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: { ...vaultData, entries: [entry], sshKeys: [key] },
    });
    vi.mocked(vaultApi.save).mockResolvedValue(new Uint8Array([1]));

    const deleted = await useVaultStore.getState().deleteSshKey("ssh-1");

    expect(deleted).toBe(true);
    expect(useVaultStore.getState().data?.sshKeys).toEqual([]);
    expect(useVaultStore.getState().data?.entries[0]?.sshKeyIds).toEqual([]);
  });
  it("unlocks through biometrics and updates the session", async () => {
    useVaultStore.setState({
      status: "locked",
      sessionId: null,
      vaultPath: "/vaults/personal",
      publicInfo,
    });
    vi.mocked(storage.readVault).mockResolvedValue(new Uint8Array([1]));
    vi.mocked(vaultApi.biometricUnlock).mockResolvedValue({
      sessionId: "session-bio",
      data: vaultData,
    });
    vi.mocked(vaultApi.inspect).mockResolvedValue(publicInfo);

    const result = await useVaultStore.getState().unlockBiometric();

    expect(result).toEqual({ ok: true });
    expect(useVaultStore.getState().status).toBe("unlocked");
    expect(useVaultStore.getState().sessionId).toBe("session-bio");
    expect(useVaultStore.getState().data).toEqual(vaultData);
  });

  it("keeps the vault locked after a failed biometric attempt", async () => {
    useVaultStore.setState({
      status: "locked",
      vaultPath: "/vaults/personal",
      publicInfo,
    });
    vi.mocked(storage.readVault).mockResolvedValue(new Uint8Array([1]));
    vi.mocked(vaultApi.biometricUnlock).mockRejectedValue(
      new VaultApiError("biometric_failed", "Отпечаток не распознан"),
    );

    const result = await useVaultStore.getState().unlockBiometric();

    expect(result).toMatchObject({ ok: false, code: "biometric_failed" });
    expect(useVaultStore.getState().status).toBe("locked");
    expect(useVaultStore.getState().data).toBeNull();
  });

  it("stores and clears the local biometric flag", async () => {
    useVaultStore.setState({
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vaults/personal",
      data: vaultData,
    });
    vi.mocked(vaultApi.biometricEnroll).mockResolvedValue(undefined);
    vi.mocked(vaultApi.biometricDisable).mockResolvedValue(undefined);

    expect(await useVaultStore.getState().enrollBiometric()).toBe(true);
    expect(isBiometricEnabled("vault-1")).toBe(true);

    expect(await useVaultStore.getState().disableBiometric()).toBe(true);
    expect(isBiometricEnabled("vault-1")).toBe(false);
  });

});
