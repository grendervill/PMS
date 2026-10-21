import { invoke } from "@tauri-apps/api/core";
import { join } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { fromBase64, toBase64 } from "@/lib/base64";
import type { VaultStorage } from "@/lib/storage/types";

const VAULT_FILE = "pms.vault";
const ATTACHMENTS_DIR = "attachments";
const BACKUPS_DIR = "backups";

async function vaultPath(folder: string) {
  return join(folder, VAULT_FILE);
}

async function blobPath(folder: string, id: string) {
  return join(folder, ATTACHMENTS_DIR, `${id}.pmsa`);
}

export const desktopStorage: VaultStorage = {
  async pickFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Выберите папку хранилища",
    });
    return typeof selected === "string" ? selected : null;
  },

  async pickFile(title) {
    const selected = await open({
      directory: false,
      multiple: false,
      title: title ?? "Выберите файл",
    });
    return typeof selected === "string" ? selected : null;
  },

  async readFile(path) {
    const response = await invoke<{ b64: string }>("storage_read_file", { path });
    return fromBase64(response.b64);
  },

  async vaultExists(folder) {
    return invoke<boolean>("storage_exists", { path: await vaultPath(folder) });
  },

  async ensureLayout(folder) {
    await invoke("storage_mkdir", { path: folder, mode: 0o700 });
    await invoke("storage_mkdir", {
      path: await join(folder, ATTACHMENTS_DIR),
      mode: 0o700,
    });
    await invoke("storage_mkdir", {
      path: await join(folder, BACKUPS_DIR),
      mode: 0o700,
    });
  },

  async readVault(folder) {
    const response = await invoke<{ b64: string }>("storage_read_file", {
      path: await vaultPath(folder),
    });
    return fromBase64(response.b64);
  },

  async writeVault(folder, bytes) {
    await invoke("storage_write_atomic", {
      path: await vaultPath(folder),
      b64: toBase64(bytes),
      mode: 0o600,
    });
  },

  async writeBackup(folder, name, bytes) {
    await invoke("storage_write_atomic", {
      path: await join(folder, BACKUPS_DIR, name),
      b64: toBase64(bytes),
      mode: 0o600,
    });
  },

  async deleteBackup(folder, name) {
    await invoke("storage_remove_file", {
      path: await join(folder, BACKUPS_DIR, name),
    });
  },

  async listBackups(folder) {
    try {
      const entries = await invoke<Array<{ name: string; isFile: boolean }>>("storage_list_dir", {
        path: await join(folder, BACKUPS_DIR),
      });
      return entries
        .filter((entry) => entry.isFile && entry.name.endsWith(".vault"))
        .map((entry) => entry.name)
        .sort()
        .reverse();
    } catch {
      return [];
    }
  },

  async statVault(folder) {
    const stat = await invoke<{ size: number; mtimeMs: number | null }>("storage_stat", {
      path: await vaultPath(folder),
    });
    return { size: stat.size, mtimeMs: stat.mtimeMs };
  },

  async exists(path) {
    return invoke<boolean>("storage_exists", { path });
  },

  async mkdir(path, mode) {
    await invoke("storage_mkdir", { path, mode });
  },

  async saveFile(path, bytes) {
    await invoke("storage_write_atomic", {
      path,
      b64: toBase64(bytes),
      mode: 0o600,
    });
  },

  async readBlob(folder, id) {
    const response = await invoke<{ b64: string }>("storage_read_file", {
      path: await blobPath(folder, id),
    });
    return fromBase64(response.b64);
  },

  async writeBlob(folder, id, bytes) {
    await invoke("storage_write_atomic", {
      path: await blobPath(folder, id),
      b64: toBase64(bytes),
      mode: 0o600,
    });
  },

  async deleteBlob(folder, id) {
    await invoke("storage_remove_file", { path: await blobPath(folder, id) });
  },
};
