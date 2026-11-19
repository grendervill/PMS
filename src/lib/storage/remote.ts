import { desktopStorage } from "@/lib/storage/desktop";
import type { VaultStorage } from "@/lib/storage/types";

/**
 * Хранилище серверного режима: обычная папка, но это кэш приложения.
 *
 * Путь кэша приходит из Rust (`remote_cache_dir`) и уже открыт в `fs_scope` как
 * каталог приложения, поэтому все `storage_*` команды здесь работают как есть.
 * Отличие одно: папку нельзя выбрать диалогом — иначе серверный режим молча
 * превратился бы в локальный.
 */
export const remoteStorage: VaultStorage = {
  ...desktopStorage,

  async pickFolder(): Promise<string | null> {
    throw {
      code: "remote_cache_path",
      message: "В серверном режиме папка хранилища не выбирается",
    };
  },
};
