import { getMode, lastRecent } from "@/lib/storage";
import { activeProfile, type ServerProfile } from "@/features/storage/profiles";

export type StartupTarget =
  | { kind: "choose" }
  | { kind: "local"; path: string | null }
  | { kind: "remote"; profile: ServerProfile };

/**
 * Что показать при запуске.
 *
 * Без сохранённого выбора приложение не открывает последний сейф молча: сначала
 * человек говорит, где сейф живёт. Серверный режим без профиля — это тоже выбор.
 */
export function startupTarget(): StartupTarget {
  const mode = getMode();
  if (!mode) return { kind: "choose" };

  if (mode === "local") {
    return { kind: "local", path: lastRecent()?.path ?? null };
  }

  const profile = activeProfile();
  return profile ? { kind: "remote", profile } : { kind: "choose" };
}
