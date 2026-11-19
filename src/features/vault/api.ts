import { invoke } from "@tauri-apps/api/core";
import { fromBase64, toBase64 } from "@/lib/base64";
import { invokeRawBytes, toBytes } from "@/lib/ipc";
import { isAndroidPlatform } from "@/lib/platform";
import type {
  RemoteProfile,
  ServerVaultMeta,
  ServerUsage,
  ServerBlob,
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
import type {
  EntropyInput,
  LoginOptions,
  PassphraseOptions,
  PasswordOptions,
  PinOptions,
  PronounceableOptions,
  WordlistInfo,
} from "@/features/generator/types";

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

/**
 * Desktop only: the bytes are the whole IPC body, the rest goes in headers.
 *
 * Android has no raw bodies, so this must stay behind `!isAndroidPlatform()`.
 * If a platform still answers `raw_unavailable`, the error is thrown to the
 * caller instead of being retried with base64 — the base64 command is the
 * deliberate fallback for Android, not a safety net for a broken assumption.
 */
async function callRaw<T>(
  command: string,
  bytes: Uint8Array,
  headers: Record<string, string> = {},
): Promise<T> {
  try {
    return await invokeRawBytes<T>(command, bytes, headers);
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * `import_parse` answers with raw JSON bytes (the Rust side streams the parsed
 * bundle instead of building it in memory twice). Tauri hands raw payloads over
 * as an ArrayBuffer; the e2e mock returns a typed array.
 */
function decodeImportPayload(raw: unknown): ImportBundle {
  if (typeof raw === "string") return JSON.parse(raw) as ImportBundle;
  if (raw instanceof Uint8Array) {
    return JSON.parse(new TextDecoder().decode(raw)) as ImportBundle;
  }
  if (raw instanceof ArrayBuffer) {
    return JSON.parse(new TextDecoder().decode(new Uint8Array(raw))) as ImportBundle;
  }
  if (Array.isArray(raw)) {
    return JSON.parse(new TextDecoder().decode(new Uint8Array(raw as number[]))) as ImportBundle;
  }
  return raw as ImportBundle;
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
  /**
   * True when the stored key can only be read after the user authenticates
   * (macOS data-protection keychain). False means the older, weaker storage is
   * in use — worth telling the user about.
   */
  keychainProtected: boolean;
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
  host: string;
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
  passwordHistory: ImportedHistoryItem[];
};

export type ImportedHistoryItem = {
  value: string;
  changedAt: string | null;
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

export type ServerProbe = { protocol: number; spkiSha256: string };
export type ServerLogin = {
  accountId: string;
  deviceId: string;
  expiresAt: string;
  vault: ServerVaultMeta | null;
};

export const vaultApi = {
  /**
   * Tells the Rust filesystem scope that this folder holds a vault, so the
   * access grant survives a restart (folders without `pms.vault` are never
   * remembered).
   */
  async rememberFolder(folder: string): Promise<void> {
    await call<void>("vault_remember_folder", { folder });
  },

  async inspect(file: Uint8Array): Promise<VaultPublicInfo> {
    // The file is already in memory (read from disk, or just created/rewrapped),
    // so desktop can hand the bytes straight to Rust.
    if (!isAndroidPlatform()) {
      return callRaw<VaultPublicInfo>("vault_inspect_raw", file);
    }
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
    if (!isAndroidPlatform()) {
      return callRaw<UnlockedVault>("vault_unlock_raw", file, {
        "x-pms-secret": JSON.stringify(secret),
      });
    }
    return call<UnlockedVault>("vault_unlock", {
      fileB64: toBase64(file),
      secret,
    });
  },

  async save(sessionId: string, data: VaultData): Promise<Uint8Array> {
    // The command answers with the sealed file as an ArrayBuffer.
    return toBytes(await call<ArrayBuffer>("vault_save", { sessionId, data }));
  },

  /**
   * Saves straight to the vault folder on desktop: the sealed file is written by
   * Rust and never crosses the IPC boundary.
   */
  async saveTo(sessionId: string, folder: string, data: VaultData): Promise<void> {
    await call<void>("vault_save_to", { sessionId, folder, data });
  },

  /**
   * Re-wraps the vault key. The sealed payload is read from `folder` by Rust:
   * the session does not keep a copy of it, so a save no longer duplicates the
   * whole file in memory.
   */
  async rewrap(
    sessionId: string,
    folder: string,
    secret: UnlockSecret,
    keepExisting = true,
  ): Promise<Uint8Array> {
    return toBytes(
      await call<ArrayBuffer>("vault_rewrap", { sessionId, folder, secret, keepExisting }),
    );
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

  async generatePassphrase(options: PassphraseOptions): Promise<string> {
    return call<string>("generate_passphrase", { options });
  },

  async generatePin(options: PinOptions): Promise<string> {
    return call<string>("generate_pin", { options });
  },

  async generatePronounceable(options: PronounceableOptions): Promise<string> {
    return call<string>("generate_pronounceable", { options });
  },

  /**
   * k-anonymity breach check: only the first 5 hex characters of the SHA-1
   * digest leave the device. Returns the number of known breaches (0 = clean).
   */
  async breachCount(password: string): Promise<number> {
    return call<number>("password_breach_count", { password });
  },

  async generatorWordlist(): Promise<WordlistInfo> {
    return call<WordlistInfo>("generator_wordlist");
  },

  /** Imports a word list from a file the user picked in the native dialog. */
  async importWordlist(path: string): Promise<WordlistInfo> {
    return call<WordlistInfo>("generator_import_wordlist", { path });
  },

  async clearWordlist(): Promise<WordlistInfo> {
    return call<WordlistInfo>("generator_clear_wordlist");
  },

  async generatorEntropy(input: EntropyInput): Promise<number> {
    return call<number>("generator_entropy", { input });
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
    return toBytes(
      await call<ArrayBuffer>("attachment_open", {
        sessionId,
        attachmentId,
        blobB64: toBase64(blob),
      }),
    );
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

  async biometricStatus(vaultId?: string): Promise<BiometricStatus> {
    return call<BiometricStatus>("biometric_status", { vaultId });
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
    const raw = await call<unknown>("import_parse", {
      fileB64: toBase64(file),
      password,
      keyfileB64: keyfile ? toBase64(keyfile) : undefined,
    });
    return decodeImportPayload(raw);
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

  // --- Серверный режим -----------------------------------------------------

  async remoteProbe(profile: RemoteProfile): Promise<ServerProbe> {
    return call<ServerProbe>("remote_probe", { profile });
  },

  async remoteFingerprint(baseUrl: string): Promise<string> {
    return call<string>("remote_fingerprint", { baseUrl });
  },

  async remoteLogin(profile: RemoteProfile, secret: UnlockSecret): Promise<ServerLogin> {
    return call<ServerLogin>("remote_login", { profile, secret });
  },

  async remoteRegister(
    profile: RemoteProfile,
    secret: UnlockSecret,
    vaultFolder?: string,
  ): Promise<ServerLogin> {
    return call<ServerLogin>("remote_register", { profile, secret, vaultFolder });
  },

  async remoteRefresh(accountId: string): Promise<ServerLogin> {
    return call<ServerLogin>("remote_refresh", { accountId });
  },

  async remoteLogout(accountId: string): Promise<boolean> {
    return call<boolean>("remote_logout", { accountId });
  },

  async remoteCacheDir(profileId: string): Promise<string> {
    return call<string>("remote_cache_dir", { profileId });
  },

  async remoteCacheClear(profileId: string): Promise<void> {
    await call<void>("remote_cache_clear", { profileId });
  },

  async remoteVaultMeta(accountId: string): Promise<ServerVaultMeta | null> {
    return call<ServerVaultMeta | null>("remote_vault_meta", { accountId });
  },

  async remotePullVault(accountId: string, folder: string): Promise<ServerVaultMeta> {
    return call<ServerVaultMeta>("remote_pull_vault", { accountId, folder });
  },

  async remotePushVault(
    accountId: string,
    folder: string,
    expectedVersion: number | null,
  ): Promise<ServerVaultMeta> {
    return call<ServerVaultMeta>("remote_push_vault", {
      accountId,
      folder,
      expectedVersion,
    });
  },

  async remoteListBlobs(accountId: string): Promise<ServerBlob[]> {
    return call<ServerBlob[]>("remote_list_blobs", { accountId });
  },

  async remoteUploadBlob(
    accountId: string,
    blobId: string,
    path: string,
  ): Promise<ServerBlob> {
    return call<ServerBlob>("remote_upload_blob", { accountId, blobId, path });
  },

  async remoteDownloadBlob(
    accountId: string,
    blobId: string,
    dest: string,
  ): Promise<ServerBlob> {
    return call<ServerBlob>("remote_download_blob", { accountId, blobId, dest });
  },

  async remoteDeleteBlob(accountId: string, blobId: string): Promise<boolean> {
    return call<boolean>("remote_delete_blob", { accountId, blobId });
  },

  async remoteDeleteAccount(accountId: string, secret: UnlockSecret): Promise<void> {
    await call<void>("remote_delete_account", { accountId, secret });
  },

  async remoteUsage(accountId: string): Promise<ServerUsage> {
    return call<ServerUsage>("remote_usage", { accountId });
  },
};
export { fromBase64, toBase64 };
