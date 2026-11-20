/**
 * Профили серверного хранилища. В localStorage — только адрес, аккаунт и
 * политика TLS: мастер-ключ и токены живут в Rust.
 */

import type { RemoteProfile, TlsPolicy } from "@/features/vault/types";

export type ServerProfile = {
  id: string;
  name: string;
  baseUrl: string;
  accountId: string;
  /** Логин на этом сервере; хранится, чтобы не спрашивать каждый раз. */
  login: string;
  tls: TlsPolicy;
  lastLoginAt: string;
};

const PROFILES_KEY = "pms.profiles.v1";
const ACTIVE_KEY = "pms.activeProfile.v1";

function storage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

function isTlsPolicy(value: unknown): value is TlsPolicy {
  if (!value || typeof value !== "object") return false;
  const policy = value as Record<string, unknown>;
  if (policy.kind === "system" || policy.kind === "insecure") return true;
  return policy.kind === "pin" && typeof policy.spkiSha256 === "string";
}

export function isServerProfile(value: unknown): value is ServerProfile {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.id === "string" &&
    typeof item.name === "string" &&
    typeof item.baseUrl === "string" &&
    typeof item.accountId === "string" &&
    typeof item.login === "string" &&
    typeof item.lastLoginAt === "string" &&
    isTlsPolicy(item.tls)
  );
}

export function listProfiles(): ServerProfile[] {
  const raw = storage()?.getItem(PROFILES_KEY);
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(isServerProfile)
      .sort((a, b) => b.lastLoginAt.localeCompare(a.lastLoginAt));
  } catch {
    return [];
  }
}

function write(profiles: ServerProfile[]): void {
  storage()?.setItem(PROFILES_KEY, JSON.stringify(profiles));
}

export function saveProfile(profile: ServerProfile): ServerProfile[] {
  const next = [profile, ...listProfiles().filter((item) => item.id !== profile.id)].sort((a, b) =>
    b.lastLoginAt.localeCompare(a.lastLoginAt),
  );
  write(next);
  return next;
}

export function removeProfile(id: string): ServerProfile[] {
  const next = listProfiles().filter((item) => item.id !== id);
  write(next);
  if (activeProfileId() === id) setActiveProfile(null);
  return next;
}

export function activeProfileId(): string | null {
  return storage()?.getItem(ACTIVE_KEY) ?? null;
}

export function setActiveProfile(id: string | null): void {
  const store = storage();
  if (!store) return;
  if (id) store.setItem(ACTIVE_KEY, id);
  else store.removeItem(ACTIVE_KEY);
}

export function activeProfile(): ServerProfile | null {
  const id = activeProfileId();
  if (!id) return null;
  return listProfiles().find((item) => item.id === id) ?? null;
}

/** Профиль для Rust: адрес и политика TLS, без имени и аккаунта. */
export function toRemoteProfile(profile: ServerProfile): RemoteProfile {
  return { baseUrl: profile.baseUrl, tls: profile.tls, login: profile.login };
}
