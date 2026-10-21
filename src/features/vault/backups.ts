import type { VaultSettings } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";

const DAY_MS = 24 * 60 * 60 * 1000;

export function backupName(date = new Date()): string {
  return `${date.toISOString().replace(/[-:.]/g, "")}.vault`;
}

export function backupDate(name: string): Date | null {
  const compact = name.replace(/\.vault$/, "");
  const match = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})(\d{3})Z$/.exec(compact);
  if (!match) return null;
  const [, year, month, day, hour, minute, second, millis] = match;
  const date = new Date(
    Date.UTC(
      Number(year),
      Number(month) - 1,
      Number(day),
      Number(hour),
      Number(minute),
      Number(second),
      Number(millis),
    ),
  );
  return Number.isNaN(date.getTime()) ? null : date;
}

export async function lastBackupAt(folder: string): Promise<Date | null> {
  const names = await getStorage().listBackups(folder);
  for (const name of names) {
    const date = backupDate(name);
    if (date) return date;
  }
  return null;
}

export async function cleanupBackups(
  folder: string,
  retentionDays: number,
): Promise<number> {
  const days = Math.max(1, retentionDays);
  const names = await getStorage().listBackups(folder);
  let removed = 0;
  for (let index = 1; index < names.length; index += 1) {
    const date = backupDate(names[index]!);
    if (!date) continue;
    if (Date.now() - date.getTime() > days * DAY_MS) {
      try {
        await getStorage().deleteBackup(folder, names[index]!);
        removed += 1;
      } catch {
        // A backup that cannot be deleted must not break saving or unlocking.
      }
    }
  }
  return removed;
}

export async function createBackup(
  folder: string,
  settings: VaultSettings,
  force = false,
): Promise<boolean> {
  if (!force && settings.autoBackupDays === 0) return false;

  const names = await getStorage().listBackups(folder);
  if (!force && names.length > 0) {
    const newest = backupDate(names[0]!);
    if (newest && Date.now() - newest.getTime() < settings.autoBackupDays * DAY_MS) {
      return false;
    }
  }

  const bytes = await getStorage().readVault(folder);
  await getStorage().writeBackup(folder, backupName(), bytes);
  await cleanupBackups(folder, settings.backupRetentionDays);
  return true;
}

export async function createManualBackup(
  folder: string,
  settings: VaultSettings,
): Promise<boolean> {
  return createBackup(folder, settings, true);
}
