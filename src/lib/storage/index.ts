import { androidStorage } from "@/lib/storage/android";
import { desktopStorage } from "@/lib/storage/desktop";
import { isAndroidPlatform } from "@/lib/platform";
import type { VaultStorage } from "@/lib/storage/types";

export function getStorage(): VaultStorage {
  return isAndroidPlatform() ? androidStorage : desktopStorage;
}

export * from "@/lib/storage/types";
export * from "@/lib/storage/recent";
