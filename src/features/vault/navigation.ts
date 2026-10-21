import { KeyRound, KeySquare, Settings2, ShieldCheck, Timer, Wand2 } from "lucide-react";

export type AppSection = "entries" | "totp" | "ssh" | "generator" | "audit" | "settings";

export const navItems = [
  { id: "entries", label: "Записи", icon: KeyRound },
  { id: "totp", label: "TOTP", icon: Timer },
  { id: "ssh", label: "SSH", icon: KeySquare },
  { id: "generator", label: "Генератор", icon: Wand2 },
  { id: "audit", label: "Проверка", icon: ShieldCheck },
  { id: "settings", label: "Настройки", icon: Settings2 },
] satisfies Array<{ id: AppSection; label: string; icon: typeof KeyRound }>;
