import { androidStorage } from "@/lib/storage/android";
import { desktopStorage } from "@/lib/storage/desktop";
import { remoteStorage } from "@/lib/storage/remote";
import { isAndroidPlatform } from "@/lib/platform";
import type { VaultStorage } from "@/lib/storage/types";
import type { StorageMode } from "@/features/storage/mode";

export function getStorage(): VaultStorage {
  return isAndroidPlatform() ? androidStorage : desktopStorage;
}

/**
 * Хранилище для выбранного режима. `folder` в обоих случаях — путь на диске:
 * в серверном режиме это каталог кэша, полученный из Rust.
 */
export function storageFor(mode: StorageMode): VaultStorage {
  if (mode === "remote") return remoteStorage;
  return getStorage();
}

export * from "@/lib/storage/types";
export { remoteStorage } from "@/lib/storage/remote";
export * from "@/lib/storage/recent";
