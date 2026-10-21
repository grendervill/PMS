import blueShield from "@/assets/appicons/01-blue-shield.png";
import goldKey from "@/assets/appicons/03-gold-key.png";
import greenLock from "@/assets/appicons/02-green-lock.png";

export type AppIconId = "blue-shield" | "green-lock" | "gold-key";

export type AppIconOption = {
  id: AppIconId;
  label: string;
  description: string;
  image: string;
};

export const APP_ICONS: AppIconOption[] = [
  {
    id: "green-lock",
    label: "Зелёный замок",
    description: "Спокойный зелёный сейф",
    image: greenLock,
  },
  {
    id: "blue-shield",
    label: "Синий щит",
    description: "Холодный синий щит",
    image: blueShield,
  },
  {
    id: "gold-key",
    label: "Золотой ключ",
    description: "Тёплый золотой ключ",
    image: goldKey,
  },
];

const STORAGE_KEY = "pms.appIcon";

export function getSelectedAppIcon(): AppIconId {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === "blue-shield" || value === "gold-key" || value === "green-lock") {
      return value;
    }
  } catch {
    // localStorage can be unavailable; the bundled icon remains active.
  }
  return "green-lock";
}

export function setSelectedAppIcon(icon: AppIconId): void {
  try {
    localStorage.setItem(STORAGE_KEY, icon);
  } catch {
    // The icon was still applied for this session.
  }
}
