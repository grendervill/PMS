import { androidStorage } from "@/lib/storage/android";
import { desktopStorage } from "@/lib/storage/desktop";
import { remoteStorage } from "@/lib/storage/remote";
import { isAndroidPlatform } from "@/lib/platform";
import type { VaultStorage } from "@/lib/storage/types";
import { getMode, type StorageMode } from "@/lib/storage/mode";

/**
 * Хранилище текущего режима. Серверный режим — та же папка, но это кэш
 * приложения, поэтому все пути к нему приходят из Rust.
 */
export function getStorage(): VaultStorage {
  return storageFor(getMode() ?? "local");
}

/**
 * Хранилище для выбранного режима. `folder` в обоих случаях — путь на диске:
 * в серверном режиме это каталог кэша, полученный из Rust.
 */
export function storageFor(mode: StorageMode): VaultStorage {
  if (mode === "remote") return remoteStorage;
  return isAndroidPlatform() ? androidStorage : desktopStorage;
}

export * from "@/lib/storage/types";
export * from "@/lib/storage/mode";
export { remoteStorage } from "@/lib/storage/remote";
export * from "@/lib/storage/recent";
