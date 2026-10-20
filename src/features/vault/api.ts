import { invoke } from "@tauri-apps/api/core";
import { fromBase64, toBase64 } from "@/lib/base64";
import type {
  KdfProfile,
  UnlockSecret,
  VaultData,
  VaultErrorInfo,
  VaultPublicInfo,
  CipherSuite,
} from "@/features/vault/types";

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

  async rewrap(
    sessionId: string,
    secret: UnlockSecret,
    keepExisting = true,
  ): Promise<Uint8Array> {
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
};

export { fromBase64, toBase64 };
