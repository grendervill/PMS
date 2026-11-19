import type { VaultStorage } from "@/lib/storage";
import type { ServerBlob } from "@/features/vault/types";

/** Блоб живёт на сервере, пока его не трогали столько дней. */
export const RETENTION_DAYS = 30;
const RETENTION_MS = RETENTION_DAYS * 24 * 60 * 60 * 1000;

export type BlobRemote = {
  remoteListBlobs(accountId: string): Promise<ServerBlob[]>;
  remoteUploadBlob(accountId: string, blobId: string, path: string): Promise<ServerBlob>;
  remoteDeleteBlob(accountId: string, blobId: string): Promise<boolean>;
};

/**
 * Удалять ли блоб с сервера.
 *
 * Сервер не разбирает сейф, поэтому не знает, какие вложения нужны. Удаляем
 * только то, чего нет локально и к чему не обращались RETENTION_DAYS дней: у
 * другого устройства может быть старая версия сейфа, и его вложение должно
 * дожить до следующей синхронизации.
 */
export function shouldDelete(blob: ServerBlob, localIds: Set<string>, now: Date): boolean {
  if (localIds.has(blob.blobId)) return false;
  const used = Date.parse(blob.lastUsedAt);
  if (Number.isNaN(used)) return false;
  return now.getTime() - used > RETENTION_MS;
}

export async function syncBlobs(deps: {
  accountId: string;
  cacheDir: string;
  storage: VaultStorage;
  remote: BlobRemote;
  now?: () => Date;
}): Promise<{ uploaded: string[]; deleted: string[]; failed: string[] }> {
  const { accountId, cacheDir, storage, remote } = deps;
  const now = deps.now ? deps.now() : new Date();

  const localIds = new Set(await storage.listBlobIds(cacheDir));
  const serverBlobs = await remote.remoteListBlobs(accountId);
  const serverIds = new Set(serverBlobs.map((item) => item.blobId));

  const uploaded: string[] = [];
  const deleted: string[] = [];
  const failed: string[] = [];

  for (const blobId of localIds) {
    if (serverIds.has(blobId)) continue;
    try {
      await remote.remoteUploadBlob(accountId, blobId, `${cacheDir}/attachments/${blobId}.pmsa`);
      uploaded.push(blobId);
    } catch {
      failed.push(blobId);
    }
  }

  for (const blob of serverBlobs) {
    if (!shouldDelete(blob, localIds, now)) continue;
    try {
      if (await remote.remoteDeleteBlob(accountId, blob.blobId)) deleted.push(blob.blobId);
    } catch {
      failed.push(blob.blobId);
    }
  }

  return { uploaded, deleted, failed };
}
