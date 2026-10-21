import { fromBase64 } from "@/lib/base64";
import { getStorage } from "@/lib/storage";
import { createEntry } from "@/features/entries/entries";
import { vaultApi, type ImportBundle, type ImportedItem } from "@/features/vault/api";
import type {
  AttachmentMeta,
  Entry,
  SshKey,
  VaultData,
} from "@/features/vault/types";

export type ImportResult = {
  entries: number;
  sshKeys: number;
  attachments: number;
  skipped: number;
  warnings: string[];
};

export type ImportOptions = {
  skipDuplicates: boolean;
};

function signature(entry: Pick<Entry, "name" | "login" | "password"> & {
  urls?: string[];
  url?: string;
}) {
  const urls = entry.urls?.length
    ? entry.urls
    : entry.url
      ? [entry.url]
      : [];
  return [
    entry.name.trim().toLowerCase(),
    entry.login,
    entry.password,
    urls.join("\n"),
  ].join("\u0000");
}

function sshType(publicKey: string): SshKey["type"] {
  if (publicKey.startsWith("ssh-rsa")) return "rsa";
  if (publicKey.startsWith("ecdsa-")) return "ecdsa";
  return "ed25519";
}

async function importAttachments(
  item: ImportedItem,
  data: VaultData,
  sessionId: string,
  vaultPath: string,
  result: ImportResult,
): Promise<{ ids: string[]; metas: AttachmentMeta[] }> {
  const ids: string[] = [];
  const metas: AttachmentMeta[] = [];
  const maxBytes = Math.max(1, data.settings.maxAttachmentMb) * 1024 * 1024;
  for (const attachment of item.attachments) {
    try {
      const bytes = fromBase64(attachment.b64);
      if (bytes.byteLength > maxBytes) {
        result.warnings.push(
          `${item.name}: вложение «${attachment.name}» больше лимита и пропущено`,
        );
        continue;
      }
      const sealed = await vaultApi.sealAttachment(
        sessionId,
        attachment.name,
        attachment.mime || "application/octet-stream",
        bytes,
      );
      await getStorage().writeBlob(vaultPath, sealed.meta.id, sealed.blob);
      ids.push(sealed.meta.id);
      metas.push(sealed.meta);
      result.attachments += 1;
    } catch {
      result.warnings.push(`${item.name}: не удалось импортировать вложение «${attachment.name}»`);
    }
  }
  return { ids, metas };
}

export async function runImport(
  bundle: ImportBundle,
  data: VaultData,
  sessionId: string,
  vaultPath: string,
  options: ImportOptions,
): Promise<{ data: VaultData; result: ImportResult }> {
  const result: ImportResult = {
    entries: 0,
    sshKeys: 0,
    attachments: 0,
    skipped: 0,
    warnings: [...bundle.warnings],
  };
  const entries = [...data.entries];
  const sshKeys = [...data.sshKeys];
  const attachments = [...data.attachments];
  const seen = new Set(entries.map((entry) => signature(entry)));
  const seenSsh = new Set(sshKeys.map((key) => key.publicKey));

  for (const item of bundle.items) {
    if (item.kind === "ssh") {
      if (!item.sshKey?.publicKey.trim() || !item.sshKey.privateKey.trim()) {
        result.warnings.push(`${item.name}: SSH-ключ без ключевого материала`);
        continue;
      }
      if (options.skipDuplicates && seenSsh.has(item.sshKey.publicKey)) {
        result.skipped += 1;
        continue;
      }
      const now = new Date().toISOString();
      sshKeys.unshift({
        id: crypto.randomUUID(),
        name: item.sshKey.name || item.name,
        type: sshType(item.sshKey.publicKey),
        rsaBits: null,
        curve: null,
        publicKey: item.sshKey.publicKey,
        fingerprint: null,
        privateKey: item.sshKey.privateKey,
        passphrase: item.sshKey.passphrase ?? null,
        comment: item.sshKey.comment || item.comment,
        attachmentIds: [],
        createdAt: now,
        updatedAt: now,
      });
      seenSsh.add(item.sshKey.publicKey);
      result.sshKeys += 1;
      continue;
    }

    const urlSignature = item.urls.join("\n");
    const importSignature = [
      item.name.trim().toLowerCase(),
      item.login,
      item.password,
      urlSignature,
    ].join("\u0000");
    if (options.skipDuplicates && seen.has(importSignature)) {
      result.skipped += 1;
      continue;
    }

    const sealed = await importAttachments(item, data, sessionId, vaultPath, result);
    const entry = createEntry(
      {
        name: item.name || "Без названия",
        login: item.login,
        password: item.password,
        urls: item.urls,
        comment: item.comment,
        attachmentIds: sealed.ids,
        sshKeyIds: [],
        totp: item.totp,
        totpRaw: "",
      },
    );
    entries.unshift(entry);
    attachments.unshift(...sealed.metas);
    seen.add(importSignature);
    result.entries += 1;
  }

  return {
    data: {
      ...data,
      entries,
      sshKeys,
      attachments,
      updatedAt: new Date().toISOString(),
    },
    result,
  };
}

export async function parseImportFile(
  bytes: Uint8Array,
  password?: string,
  keyfile?: Uint8Array,
): Promise<ImportBundle> {
  return vaultApi.importParse(bytes, password, keyfile);
}
