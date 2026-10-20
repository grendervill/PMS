export type RecentVault = {
  path: string;
  name: string;
  vaultId: string;
  lastOpenedAt: string;
};

export type VaultStorage = {
  pickFolder(): Promise<string | null>;
  pickFile(): Promise<string | null>;
  readFile(path: string): Promise<Uint8Array>;
  vaultExists(folder: string): Promise<boolean>;
  ensureLayout(folder: string): Promise<void>;
  readVault(folder: string): Promise<Uint8Array>;
  writeVault(folder: string, bytes: Uint8Array): Promise<void>;
  writeBackup(folder: string, name: string, bytes: Uint8Array): Promise<void>;
  listBackups(folder: string): Promise<string[]>;
  statVault(folder: string): Promise<{ size: number; mtimeMs: number | null }>;
  exists(path: string): Promise<boolean>;
  mkdir(path: string, mode?: number): Promise<void>;
  saveFile(path: string, bytes: Uint8Array): Promise<void>;
  readBlob(folder: string, id: string): Promise<Uint8Array>;
  writeBlob(folder: string, id: string, bytes: Uint8Array): Promise<void>;
  deleteBlob(folder: string, id: string): Promise<void>;
};
