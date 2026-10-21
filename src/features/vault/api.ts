import { invoke } from "@tauri-apps/api/core";
import { fromBase64, toBase64 } from "@/lib/base64";
import type {
  KdfProfile,
  UnlockSecret,
  VaultData,
  VaultErrorInfo,
  VaultPublicInfo,
  CipherSuite,
  AttachmentMeta,
  TotpConfig,
  SshKey,
  SshKeyType,
} from "@/features/vault/types";
import type { LoginOptions, PasswordOptions } from "@/features/generator/types";

export class VaultApiError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "VaultApiError";
    this.code = code;
  }
}

function normalizeError(error: unknown): VaultApiError {
  if (error instanceof VaultApiError) return error;
  if (error && typeof error === "object") {
    const candidate = error as Partial<VaultErrorInfo>;
    if (typeof candidate.code === "string" && typeof candidate.message === "string") {
      return new VaultApiError(candidate.code, candidate.message);
    }
  }
  if (error instanceof Error) {
    return new VaultApiError("unknown", error.message);
  }
  return new VaultApiError("unknown", "Не удалось выполнить операцию");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export type VaultConfig = {
  name: string;
  kdfProfile: KdfProfile;
  cipher: CipherSuite;
};

export type CreatedVault = {
  sessionId: string;
  data: VaultData;
  file: Uint8Array;
};

export type UnlockedVault = {
  sessionId: string;
  data: VaultData;
};

export type BiometricStatus = {
  available: boolean;
};

export type SharedCard = {
  name: string;
  login: string;
  password: string;
  urls: string[];
  comment: string;
  totp: TotpConfig | null;
};

export type TransferPeer = {
  id: string;
  name: string;
  host: string;
  port: number;
};

export type ReceiveStarted = {
  code: string;
  deviceName: string;
  port: number;
};

export type ReceivedArchive = {
  archivePath: string;
  sender: string;
  destination: string;
  replaceCurrent: boolean;
};

export type ImportedAttachment = {
  name: string;
  mime: string;
  b64: string;
};

export type ImportedSshKey = {
  name: string;
  publicKey: string;
  privateKey: string;
  passphrase?: string | null;
  comment: string;
};

export type ImportedItem = {
  kind: "login" | "note" | "card" | "identity" | "ssh";
  name: string;
  login: string;
  password: string;
  urls: string[];
  comment: string;
  totp: TotpConfig | null;
  attachments: ImportedAttachment[];
  sshKey?: ImportedSshKey | null;
};

export type ImportBundle = {
  format: string;
  items: ImportedItem[];
  warnings: string[];
};

export type SshGenerateOptions = {
  keyType: SshKeyType;
  rsaBits?: number | null;
  curve?: string | null;
  comment: string;
  passphrase?: string | null;
};

export type GeneratedSshKey = {
  keyType: SshKeyType;
  rsaBits?: number | null;
  curve?: string | null;
  publicKey: string;
  privateKey: string;
  fingerprint: string;
};

export type SshFormats = {
  publicKey: string;
  openssh: string;
  pem: string;
  pkcs8: string;
  fingerprint: string;
};

export type InstalledSshKey = {
  privatePath: string;
  publicPath: string;
  privateMode: number;
  publicMode: number;
  directoryMode: number;
  ownedByCurrentUser: boolean;
  warning?: string | null;
};

export type SshPermissionStatus = {
  exists: boolean;
  privateMode?: number | null;
  publicMode?: number | null;
  directoryMode?: number | null;
  ownedByCurrentUser: boolean;
  ok: boolean;
};

export type SshKeyDraft = Omit<SshKey, "id" | "createdAt" | "updatedAt">;

export const vaultApi = {
  async inspect(file: Uint8Array): Promise<VaultPublicInfo> {
    return call<VaultPublicInfo>("vault_inspect", { fileB64: toBase64(file) });
  },

  async create(config: VaultConfig, secret: UnlockSecret): Promise<CreatedVault> {
    const response = await call<{
      sessionId: string;
      fileB64: string;
      data: VaultData;
    }>("vault_create", { config, secret });
    return {
      sessionId: response.sessionId,
      data: response.data,
      file: fromBase64(response.fileB64),
    };
  },

  async unlock(file: Uint8Array, secret: UnlockSecret): Promise<UnlockedVault> {
    return call<UnlockedVault>("vault_unlock", {
      fileB64: toBase64(file),
      secret,
    });
  },

  async save(sessionId: string, data: VaultData): Promise<Uint8Array> {
    const response = await call<{ fileB64: string }>("vault_save", {
      sessionId,
      data,
    });
    return fromBase64(response.fileB64);
  },

  async rewrap(sessionId: string, secret: UnlockSecret, keepExisting = true): Promise<Uint8Array> {
    const response = await call<{ fileB64: string }>("vault_rewrap", {
      sessionId,
      secret,
      keepExisting,
    });
    return fromBase64(response.fileB64);
  },

  async lock(sessionId: string): Promise<void> {
    await call<void>("vault_lock", { sessionId });
  },

  async generatePassword(options: PasswordOptions): Promise<string> {
    return call<string>("generate_password", { options });
  },

  async generateLogin(options: LoginOptions): Promise<string> {
    return call<string>("generate_login", { options });
  },

  async sealAttachment(
    sessionId: string,
    name: string,
    mime: string,
    bytes: Uint8Array,
  ): Promise<{ blob: Uint8Array; meta: AttachmentMeta }> {
    const response = await call<{ blobB64: string; meta: AttachmentMeta }>("attachment_seal", {
      sessionId,
      name,
      mime,
      b64: toBase64(bytes),
    });
    return { blob: fromBase64(response.blobB64), meta: response.meta };
  },

  async openAttachment(
    sessionId: string,
    attachmentId: string,
    blob: Uint8Array,
  ): Promise<Uint8Array> {
    const response = await call<{ b64: string }>("attachment_open", {
      sessionId,
      attachmentId,
      blobB64: toBase64(blob),
    });
    return fromBase64(response.b64);
  },

  async inspectTotp(input: string): Promise<TotpConfig> {
    return call<TotpConfig>("totp_inspect", { input });
  },

  async totpCode(config: TotpConfig): Promise<{ code: string; remaining: number }> {
    return call<{ code: string; remaining: number }>("totp_code", { config });
  },

  async sshGenerate(options: SshGenerateOptions): Promise<GeneratedSshKey> {
    return call<GeneratedSshKey>("ssh_generate", { options });
  },

  async sshFormats(privateKey: string, passphrase?: string): Promise<SshFormats> {
    return call<SshFormats>("ssh_formats", { privateKey, passphrase });
  },

  async sshInstall(
    directory: string,
    name: string,
    privateKey: string,
    publicKey: string,
    overwrite: boolean,
  ): Promise<InstalledSshKey> {
    return call<InstalledSshKey>("ssh_install", {
      directory,
      name,
      privateKey,
      publicKey,
      overwrite,
    });
  },

  async sshCheck(directory: string, name: string): Promise<SshPermissionStatus> {
    return call<SshPermissionStatus>("ssh_check", { directory, name });
  },

  async keyfileKind(file: Uint8Array): Promise<"ssh" | "public" | "file"> {
    return call<"ssh" | "public" | "file">("keyfile_kind", {
      fileB64: toBase64(file),
    });
  },

  async biometricStatus(): Promise<BiometricStatus> {
    return call<BiometricStatus>("biometric_status");
  },

  async biometricEnroll(sessionId: string): Promise<void> {
    await call<void>("biometric_enroll", { sessionId });
  },

  async biometricDisable(vaultId: string): Promise<void> {
    await call<void>("biometric_disable", { vaultId });
  },

  async biometricUnlock(file: Uint8Array, vaultId: string): Promise<UnlockedVault> {
    return call<UnlockedVault>("biometric_unlock", {
      fileB64: toBase64(file),
      vaultId,
    });
  },

  async shareEncode(card: SharedCard, password?: string): Promise<string> {
    return call<string>("share_encode", { card, password });
  },

  async shareDecode(payload: string, password?: string): Promise<SharedCard> {
    return call<SharedCard>("share_decode", { payload, password });
  },

  async importParse(
    file: Uint8Array,
    password?: string,
    keyfile?: Uint8Array,
  ): Promise<ImportBundle> {
    return call<ImportBundle>("import_parse", {
      fileB64: toBase64(file),
      password,
      keyfileB64: keyfile ? toBase64(keyfile) : undefined,
    });
  },

  async pmsExport(folder: string, destination: string): Promise<void> {
    await call<void>("pms_export", { folder, destination });
  },

  async pmsImport(archive: string, destination: string): Promise<string> {
    return call<string>("pms_import", { archive, destination });
  },

  async setAppIcon(icon: string): Promise<void> {
    await call<void>("app_icon_set", { icon });
  },

  async transferScan(durationMs?: number): Promise<TransferPeer[]> {
    return call<TransferPeer[]>("transfer_scan", { durationMs });
  },

  async transferReceiveStart(
    destination: string,
    replaceCurrent: boolean,
  ): Promise<ReceiveStarted> {
    return call<ReceiveStarted>("transfer_receive_start", {
      destination,
      replaceCurrent,
    });
  },

  async transferReceiveStop(): Promise<void> {
    await call<void>("transfer_receive_stop");
  },

  async transferSend(host: string, port: number, code: string, vaultPath: string): Promise<void> {
    await call<void>("transfer_send", { host, port, code, vaultPath });
  },

  async transferInstall(
    archivePath: string,
    destination: string,
    replaceCurrent: boolean,
  ): Promise<string> {
    return call<string>("transfer_install", {
      archivePath,
      destination,
      replaceCurrent,
    });
  },
};
export { fromBase64, toBase64 };
