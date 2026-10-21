import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { fromBase64, toBase64 } from "@/lib/base64";
import { getStorage } from "@/lib/storage";
import { isAndroidPlatform } from "@/lib/platform";
import { vaultApi } from "@/features/vault/api";
import { backupName } from "@/features/vault/backups";

type DirEntry = {
  name: string;
  path: string;
  isFile: boolean;
};

async function writeCacheFile(path: string, bytes: Uint8Array) {
  await invoke("storage_write_atomic", {
    path,
    b64: toBase64(bytes),
    mode: 0o600,
  });
}

async function readCacheFile(path: string): Promise<Uint8Array> {
  const response = await invoke<{ b64: string }>("storage_read_file", { path });
  return fromBase64(response.b64);
}

async function cacheDir(name: string) {
  const cache = await appCacheDir();
  return join(cache, `pms-${name}-${crypto.randomUUID()}`);
}

async function cleanupFiles(paths: string[]) {
  await Promise.all(
    paths.map((path) => invoke("storage_remove_file", { path }).catch(() => undefined)),
  );
}

export async function cleanupCacheDirectory(path: string) {
  await invoke("storage_remove_dir", { path }).catch(() => undefined);
}

export async function exportVaultArchive(
  folder: string,
  suggestedName: string,
): Promise<string | null> {
  const storage = getStorage();
  const destination = await storage.pickSaveFile(suggestedName, "application/zip");
  if (!destination) return null;

  if (!isAndroidPlatform()) {
    await vaultApi.pmsExport(folder, destination);
    return destination;
  }

  const staging = await cacheDir("export");
  const archive = await cacheDir("archive.pmsvault");
  const cachedFiles: string[] = [];
  try {
    await invoke("storage_mkdir", { path: await join(staging, "attachments"), mode: 0o700 });
    const vault = await storage.readVault(folder);
    const cachedVault = await join(staging, "pms.vault");
    await writeCacheFile(cachedVault, vault);
    cachedFiles.push(cachedVault);

    for (const id of await storage.listBlobIds(folder)) {
      const blob = await storage.readBlob(folder, id);
      const path = await join(staging, "attachments", `${id}.pmsa`);
      await writeCacheFile(path, blob);
      cachedFiles.push(path);
    }

    await vaultApi.pmsExport(staging, archive);
    cachedFiles.push(archive);
    await storage.saveFile(destination, await readCacheFile(archive));
    return destination;
  } finally {
    await cleanupFiles(cachedFiles);
    await cleanupCacheDirectory(staging);
  }
}

export async function importVaultArchive(
  archive: string,
  destination: string,
): Promise<string> {
  const storage = getStorage();
  if (!isAndroidPlatform()) {
    await vaultApi.pmsImport(archive, destination);
    return destination;
  }

  if (await storage.vaultExists(destination)) {
    throw { code: "vault_exists", message: "В выбранной папке уже есть хранилище" };
  }

  const staging = await cacheDir("import");
  const cachedArchive = await cacheDir("archive.pmsvault");
  const cachedFiles: string[] = [cachedArchive, await join(staging, "pms.vault")];
  try {
    await writeCacheFile(cachedArchive, await storage.readFile(archive));
    await vaultApi.pmsImport(cachedArchive, staging);

    await storage.ensureLayout(destination);
    await storage.writeVault(destination, await readCacheFile(await join(staging, "pms.vault")));

    const attachments = await invoke<DirEntry[]>("storage_list_dir", {
      path: await join(staging, "attachments"),
    }).catch(() => []);
    for (const entry of attachments) {
      if (!entry.isFile || !entry.name.endsWith(".pmsa")) continue;
      const id = entry.name.slice(0, -".pmsa".length);
      await storage.writeBlob(destination, id, await readCacheFile(entry.path));
      cachedFiles.push(entry.path);
    }
    return destination;
  } finally {
    await cleanupFiles(cachedFiles);
    await cleanupCacheDirectory(staging);
  }
}

export async function stageVaultForTransfer(folder: string): Promise<string> {
  const staging = await cacheDir("transfer-send");
  try {
    await invoke("storage_mkdir", { path: await join(staging, "attachments"), mode: 0o700 });
    await writeCacheFile(await join(staging, "pms.vault"), await getStorage().readVault(folder));
    for (const id of await getStorage().listBlobIds(folder)) {
      await writeCacheFile(
        await join(staging, "attachments", `${id}.pmsa`),
        await getStorage().readBlob(folder, id),
      );
    }
    return staging;
  } catch (error) {
    await cleanupCacheDirectory(staging);
    throw error;
  }
}

export async function installNativeArchive(
  archive: string,
  destination: string,
  replaceCurrent: boolean,
): Promise<string> {
  const storage = getStorage();
  if (!replaceCurrent && (await storage.vaultExists(destination))) {
    throw { code: "vault_exists", message: "В выбранной папке уже есть хранилище" };
  }
  if (replaceCurrent) {
    const current = await storage.readVault(destination);
    await storage.writeBackup(destination, backupName(), current);
  }

  const staging = await cacheDir("transfer-install");
  const cachedFiles: string[] = [archive];
  try {
    await vaultApi.pmsImport(archive, staging);
    await storage.ensureLayout(destination);
    await storage.writeVault(destination, await readCacheFile(await join(staging, "pms.vault")));
    const attachments = await invoke<DirEntry[]>("storage_list_dir", {
      path: await join(staging, "attachments"),
    }).catch(() => []);
    for (const entry of attachments) {
      if (!entry.isFile || !entry.name.endsWith(".pmsa")) continue;
      const id = entry.name.slice(0, -".pmsa".length);
      await storage.writeBlob(destination, id, await readCacheFile(entry.path));
      cachedFiles.push(entry.path);
    }
    return destination;
  } finally {
    await cleanupFiles(cachedFiles);
    await cleanupCacheDirectory(staging);
  }
}
