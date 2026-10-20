import { KeyRound, Settings2, ShieldCheck, Wand2 } from "lucide-react";

export type AppSection = "entries" | "generator" | "audit" | "settings";

export const navItems = [
  { id: "entries", label: "Записи", icon: KeyRound },
  { id: "generator", label: "Генератор", icon: Wand2 },
  { id: "audit", label: "Проверка", icon: ShieldCheck },
  { id: "settings", label: "Настройки", icon: Settings2 },
] satisfies Array<{ id: AppSection; label: string; icon: typeof KeyRound }>;
