import { desktopStorage } from "@/lib/storage/desktop";
import type { VaultStorage } from "@/lib/storage/types";

export function getStorage(): VaultStorage {
  return desktopStorage;
}

export * from "@/lib/storage/types";
export * from "@/lib/storage/recent";
