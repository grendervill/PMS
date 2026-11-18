import { invoke } from "@tauri-apps/api/core";
import { join } from "@tauri-apps/api/path";
import { toBase64 } from "@/lib/base64";
import { toBytes } from "@/lib/ipc";
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
  // The pickers live in Rust: they are also the only way to grant the app access
  // to a path (see `commands/fs_scope.rs`), so the renderer can never widen its
  // own permissions.
  async pickFolder() {
    return invoke<string | null>("dialog_pick_folder", {
      title: "Выберите папку хранилища",
    });
  },

  async pickFile(title) {
    return invoke<string | null>("dialog_pick_file", {
      title: title ?? "Выберите файл",
    });
  },

  async pickSaveFile(defaultName) {
    return invoke<string | null>("dialog_pick_save_file", {
      suggestedName: defaultName,
      title: "Сохранить файл",
    });
  },

  displayName(value) {
    const parts = value.split(/[\\/]/).filter(Boolean);
    return parts.at(-1) ?? "Хранилище";
  },

  async readFile(path) {
    // `storage_read_file` answers with an ArrayBuffer: base64 cost ~70 ms per
    // 4 MiB in JavaScript and inflated the wire format by a third.
    return toBytes(await invoke<ArrayBuffer>("storage_read_file", { path }));
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
    return toBytes(
      await invoke<ArrayBuffer>("storage_read_file", { path: await vaultPath(folder) }),
    );
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
    return toBytes(
      await invoke<ArrayBuffer>("storage_read_file", { path: await blobPath(folder, id) }),
    );
  },

  async sealAttachment(sessionId, folder, name, mime, bytes) {
    // The sealed blob is written by Rust: sending it back here cost three full
    // base64 round trips per upload.
    const response = await invoke<{ meta: import("@/features/vault/types").AttachmentMeta }>(
      "attachment_seal_store",
      { sessionId, folder, name, mime, b64: toBase64(bytes) },
    );
    return response.meta;
  },

  async openAttachment(sessionId, folder, id) {
    return toBytes(
      await invoke<ArrayBuffer>("attachment_open_file", { sessionId, attachmentId: id, folder }),
    );
  },

  async writeBlob(folder, id, bytes) {
    await invoke("storage_write_atomic", {
      path: await blobPath(folder, id),
      b64: toBase64(bytes),
      mode: 0o600,
    });
  },

  async listBlobIds(folder) {
    try {
      const entries = await invoke<Array<{ name: string; isFile: boolean }>>("storage_list_dir", {
        path: await join(folder, ATTACHMENTS_DIR),
      });
      return entries
        .filter((entry) => entry.isFile && entry.name.endsWith(".pmsa"))
        .map((entry) => entry.name.slice(0, -".pmsa".length));
    } catch {
      return [];
    }
  },

  async deleteBlob(folder, id) {
    await invoke("storage_remove_file", { path: await blobPath(folder, id) });
  },
};
