import {
  createDir,
  createNewFile,
  getMetadata,
  getName,
  persistPickerUriPermission,
  readDir,
  readFile as readAndroidFile,
  removeFile,
  showOpenDirPicker,
  showOpenFilePicker,
  showSaveFilePicker,
  writeFile,
  type EntryMetadataWithUri,
  type FsUri,
} from "tauri-plugin-android-fs-api";
import { fromBase64, toBase64 } from "@/lib/base64";
import type { VaultStorage } from "@/lib/storage/types";

const VAULT_FILE = "pms.vault";
const ATTACHMENTS_DIR = "attachments";
const BACKUPS_DIR = "backups";
const FOLDER_PREFIX = "pms-android-folder:";
const FILE_PREFIX = "pms-android-file:";

type AndroidFolder = {
  uri: FsUri;
  name: string;
};

type AndroidFile = {
  uri: FsUri;
  name: string;
};

function encodeToken<T>(prefix: string, value: T): string {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  return `${prefix}${toBase64(bytes)}`;
}

function decodeToken<T>(prefix: string, value: string): T | null {
  if (!value.startsWith(prefix)) return null;
  try {
    const bytes = fromBase64(value.slice(prefix.length));
    return JSON.parse(new TextDecoder().decode(bytes)) as T;
  } catch {
    return null;
  }
}

function folderToken(value: AndroidFolder): string {
  return encodeToken(FOLDER_PREFIX, value);
}

function fileToken(value: AndroidFile): string {
  return encodeToken(FILE_PREFIX, value);
}

function folderFromToken(value: string): AndroidFolder {
  const parsed = decodeToken<AndroidFolder>(FOLDER_PREFIX, value);
  if (parsed?.uri?.uri) return parsed;
  if (value.startsWith("content://")) {
    return { uri: { uri: value, documentTopTreeUri: null }, name: "Хранилище" };
  }
  throw { code: "storage", message: "Папка хранилища недоступна. Выберите её заново." };
}

function fileFromToken(value: string): AndroidFile {
  const parsed = decodeToken<AndroidFile>(FILE_PREFIX, value);
  if (parsed?.uri?.uri) return parsed;
  if (value.startsWith("content://")) {
    return { uri: { uri: value, documentTopTreeUri: null }, name: value };
  }
  throw { code: "storage", message: "Файл недоступен. Выберите его заново." };
}

function entryUri(entry: EntryMetadataWithUri, parent: FsUri): FsUri {
  return {
    uri: entry.uri.uri,
    documentTopTreeUri:
      entry.uri.documentTopTreeUri ?? parent.documentTopTreeUri ?? null,
  };
}

async function findChild(parent: FsUri, name: string): Promise<FsUri | null> {
  const entries = await readDir(parent);
  const entry = entries.find((item) => item.name === name);
  return entry ? entryUri(entry, parent) : null;
}

async function resolveOrCreate(
  parent: FsUri,
  name: string,
  mimeType: string | null,
): Promise<FsUri> {
  const existing = await findChild(parent, name);
  if (existing) return existing;
  try {
    return await createNewFile(parent, name, mimeType);
  } catch {
    // A concurrent creation may have won; resolve again before failing.
    const raced = await findChild(parent, name);
    if (raced) return raced;
    throw { code: "storage", message: "Не удалось создать файл в выбранной папке." };
  }
}

async function resolveDir(parent: FsUri, name: string): Promise<FsUri | null> {
  return findChild(parent, name);
}

async function ensureDir(parent: FsUri, name: string): Promise<FsUri> {
  return createDir(parent, name);
}

async function readFileAt(parent: FsUri, name: string): Promise<Uint8Array> {
  const child = await findChild(parent, name);
  if (!child) {
    throw { code: "storage", message: "Файл не найден" };
  }
  return readAndroidFile(child);
}

async function writeFileAt(
  parent: FsUri,
  name: string,
  bytes: Uint8Array,
  mimeType: string | null,
): Promise<void> {
  const child = await resolveOrCreate(parent, name, mimeType);
  await writeFile(child, bytes);
}

async function removeChild(parent: FsUri, name: string): Promise<void> {
  const child = await findChild(parent, name);
  if (child) await removeFile(child);
}

async function metadataAt(parent: FsUri, name: string) {
  const child = await findChild(parent, name);
  if (!child) throw { code: "storage", message: "Файл не найден" };
  return getMetadata(child);
}

export const androidStorage: VaultStorage = {
  async pickFolder() {
    const selected = await showOpenDirPicker();
    if (!selected) return null;
    await persistPickerUriPermission(selected).catch(() => undefined);
    const name = await getName(selected).catch(() => "Хранилище");
    return folderToken({ uri: selected, name });
  },

  async pickFile(title) {
    const selected = await showOpenFilePicker({
      multiple: false,
      mimeTypes: "*/*",
    });
    const file = selected[0];
    if (!file) return null;
    const name = await getName(file).catch(() => title ?? "Файл");
    return fileToken({ uri: file, name });
  },

  async pickSaveFile(defaultName) {
    const selected = await showSaveFilePicker(defaultName, null);
    if (!selected) return null;
    const name = await getName(selected).catch(() => defaultName);
    return fileToken({ uri: selected, name });
  },

  displayName(value) {
    const folder = decodeToken<AndroidFolder>(FOLDER_PREFIX, value);
    if (folder?.name) return folder.name;
    const file = decodeToken<AndroidFile>(FILE_PREFIX, value);
    if (file?.name) return file.name;
    const parts = value.split(/[\\/]/).filter(Boolean);
    return parts.at(-1) ?? "Хранилище";
  },

  async readFile(path) {
    return readAndroidFile(fileFromToken(path).uri);
  },

  async vaultExists(folder) {
    return (await findChild(folderFromToken(folder).uri, VAULT_FILE)) !== null;
  },

  async ensureLayout(folder) {
    const root = folderFromToken(folder).uri;
    await ensureDir(root, ATTACHMENTS_DIR);
    await ensureDir(root, BACKUPS_DIR);
  },

  async readVault(folder) {
    return readFileAt(folderFromToken(folder).uri, VAULT_FILE);
  },

  async writeVault(folder, bytes) {
    const root = folderFromToken(folder).uri;
    await ensureDir(root, ATTACHMENTS_DIR);
    await ensureDir(root, BACKUPS_DIR);
    await writeFileAt(root, VAULT_FILE, bytes, "application/octet-stream");
  },

  async writeBackup(folder, name, bytes) {
    const root = folderFromToken(folder).uri;
    const backups = await ensureDir(root, BACKUPS_DIR);
    await writeFileAt(backups, name, bytes, "application/octet-stream");
  },

  async listBackups(folder) {
    const root = folderFromToken(folder).uri;
    const backups = await resolveDir(root, BACKUPS_DIR);
    if (!backups) return [];
    const entries = await readDir(backups);
    return entries
      .filter((entry) => entry.type === "File" && entry.name.endsWith(".vault"))
      .map((entry) => entry.name)
      .sort()
      .reverse();
  },

  async deleteBackup(folder, name) {
    const root = folderFromToken(folder).uri;
    const backups = await resolveDir(root, BACKUPS_DIR);
    if (backups) await removeChild(backups, name);
  },

  async statVault(folder) {
    const metadata = await metadataAt(folderFromToken(folder).uri, VAULT_FILE);
    if (metadata.type !== "File") throw { code: "storage", message: "Повреждённый vault" };
    return {
      size: metadata.byteLength,
      mtimeMs: metadata.lastModified.getTime(),
    };
  },

  async exists(path) {
    try {
      await getMetadata(fileFromToken(path).uri);
      return true;
    } catch {
      return false;
    }
  },

  async mkdir(path) {
    if (path.startsWith(FOLDER_PREFIX) || path.startsWith("content://")) return;
  },

  async saveFile(path, bytes) {
    await writeFile(fileFromToken(path).uri, bytes);
  },

  async readBlob(folder, id) {
    const root = folderFromToken(folder).uri;
    const attachments = await resolveDir(root, ATTACHMENTS_DIR);
    if (!attachments) throw { code: "storage", message: "Вложение не найдено" };
    return readFileAt(attachments, `${id}.pmsa`);
  },

  async writeBlob(folder, id, bytes) {
    const root = folderFromToken(folder).uri;
    const attachments = await ensureDir(root, ATTACHMENTS_DIR);
    await writeFileAt(attachments, `${id}.pmsa`, bytes, "application/octet-stream");
  },

  async listBlobIds(folder) {
    const root = folderFromToken(folder).uri;
    const attachments = await resolveDir(root, ATTACHMENTS_DIR);
    if (!attachments) return [];
    const entries = await readDir(attachments);
    return entries
      .filter((entry) => entry.type === "File" && entry.name.endsWith(".pmsa"))
      .map((entry) => entry.name.slice(0, -".pmsa".length));
  },

  async deleteBlob(folder, id) {
    const root = folderFromToken(folder).uri;
    const attachments = await resolveDir(root, ATTACHMENTS_DIR);
    if (attachments) await removeChild(attachments, `${id}.pmsa`);
  },
};
