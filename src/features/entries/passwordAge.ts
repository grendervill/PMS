import type { Entry } from "@/features/vault/types";

/** Sites and auditors expect a password to be replaced at least this often. */
export const PASSWORD_ROTATION_DAYS = 180;

const MS_PER_DAY = 24 * 60 * 60 * 1000;

/**
 * When the current password was set. The newest history record holds the
 * previous password with the moment it was replaced, which is exactly the
 * moment the current one appeared; `updatedAt` is the fallback for entries
 * saved before password history existed.
 */
export function passwordChangedAt(entry: Entry): string | null {
  if (!entry.password) return null;

  let newest = 0;
  let found = false;
  for (const item of entry.passwordHistory ?? []) {
    const time = Date.parse(item.changedAt);
    if (Number.isNaN(time)) continue;
    found = true;
    if (time > newest) newest = time;
  }

  if (found) return new Date(newest).toISOString();
  return Number.isNaN(Date.parse(entry.updatedAt)) ? null : entry.updatedAt;
}

/** Age of the current password in whole days, or `null` when there is none. */
export function passwordAgeDays(entry: Entry, now: Date = new Date()): number | null {
  if (!entry.password) return null;
  const changedAt = passwordChangedAt(entry);
  if (!changedAt) return null;
  const time = Date.parse(changedAt);
  if (Number.isNaN(time)) return null;
  return Math.max(0, Math.floor((now.getTime() - time) / MS_PER_DAY));
}

export function passwordNeedsRotation(entry: Entry, now: Date = new Date()): boolean {
  const age = passwordAgeDays(entry, now);
  return age !== null && age >= PASSWORD_ROTATION_DAYS;
}

export function formatDays(days: number): string {
  const mod10 = days % 10;
  const mod100 = days % 100;
  if (mod10 === 1 && mod100 !== 11) return "день";
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return "дня";
  return "дней";
}
