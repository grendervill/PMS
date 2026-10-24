import {
  ArrowLeftRight,
  CreditCard,
  IdCard,
  KeyRound,
  KeySquare,
  Settings2,
  ShieldCheck,
  Timer,
  Wand2,
} from "lucide-react";

export type AppSection =
  | "entries"
  | "cards"
  | "documents"
  | "totp"
  | "ssh"
  | "transfer"
  | "generator"
  | "audit"
  | "settings";

export const navItems = [
  { id: "entries", label: "Записи", icon: KeyRound },
  { id: "cards", label: "Карты", icon: CreditCard },
  { id: "documents", label: "Документы", icon: IdCard },
  { id: "totp", label: "TOTP", icon: Timer },
  { id: "ssh", label: "SSH", icon: KeySquare },
  { id: "transfer", label: "Обмен", icon: ArrowLeftRight },
  { id: "generator", label: "Генератор", icon: Wand2 },
  { id: "audit", label: "Проверка", icon: ShieldCheck },
  { id: "settings", label: "Настройки", icon: Settings2 },
] satisfies Array<{ id: AppSection; label: string; icon: typeof KeyRound }>;
