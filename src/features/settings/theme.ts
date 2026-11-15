/**
 * Оформление интерфейса. Темы — это только наборы токенов в `tokens.css`,
 * поэтому одна и та же тема применяется и на десктопе, и в Android-сборке.
 */

export type ThemeId =
  | "forest"
  | "light"
  | "archive"
  | "steel"
  | "terminal"
  | "linen"
  | "amber"
  | "bitwarden";

export type ThemeOption = {
  id: ThemeId;
  label: string;
  description: string;
  /** Фон, поверхность и акцент — для превью в настройках. */
  swatch: [string, string, string];
  scheme: "dark" | "light";
};

export const THEMES: ThemeOption[] = [
  {
    id: "forest",
    label: "Хвойный",
    description: "Тёмный зелёный сейф. Оформление по умолчанию.",
    swatch: ["#0f1512", "#1b2420", "#43a47c"],
    scheme: "dark",
  },
  {
    id: "light",
    label: "Светлая",
    description: "Тёплая бумага и зелёный акцент.",
    swatch: ["#f2eee4", "#ece6d8", "#2f7f60"],
    scheme: "light",
  },
  {
    id: "archive",
    label: "Архив",
    description: "Бумага, чернила и сургуч: острые углы, книжная антиква.",
    swatch: ["#efe9dc", "#e6dfd0", "#7a2e24"],
    scheme: "light",
  },
  {
    id: "steel",
    label: "Сейф",
    description: "Графитовая сталь и сигнальный оранжевый, без засечек.",
    swatch: ["#14171a", "#22272c", "#e8693a"],
    scheme: "dark",
  },
  {
    id: "terminal",
    label: "Терминал",
    description: "Почти чёрный фон, фосфорный зелёный, моноширинный шрифт.",
    swatch: ["#07090b", "#131a1e", "#4be38a"],
    scheme: "dark",
  },
  {
    id: "linen",
    label: "Лён",
    description: "Светлый и тихий, прохладные оттенки и пыльная слива.",
    swatch: ["#f1f1ef", "#e7e7e4", "#6b5b7b"],
    scheme: "light",
  },
  {
    id: "amber",
    label: "Янтарь",
    description: "Тёмное дерево и латунь, книжная антиква в заголовках.",
    swatch: ["#17120d", "#271f17", "#d9a441"],
    scheme: "dark",
  },
  {
    id: "bitwarden",
    label: "Bitwarden",
    description: "Как в веб-приложении Bitwarden: белые карточки, синий #175DDC, Inter.",
    swatch: ["#f7f9fa", "#ffffff", "#175ddc"],
    scheme: "light",
  },
];

const STORAGE_KEY = "pms.theme";

export function isThemeId(value: string | null | undefined): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

export function getSelectedTheme(): ThemeId {
  try {
    const fromUrl = new URLSearchParams(window.location.search).get("theme");
    if (isThemeId(fromUrl)) return fromUrl;
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isThemeId(stored)) return stored;
  } catch {
    // localStorage или URL могут быть недоступны — остаётся тема по умолчанию.
  }
  return "forest";
}

export function applyTheme(theme: ThemeId): void {
  const root = document.documentElement;
  if (theme === "forest") {
    root.removeAttribute("data-theme");
  } else {
    root.dataset.theme = theme;
  }
  const scheme = THEMES.find((option) => option.id === theme)?.scheme ?? "dark";
  root.style.colorScheme = scheme;
}

export function setSelectedTheme(theme: ThemeId): void {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Не смогли сохранить — применяем хотя бы на текущую сессию.
  }
  applyTheme(theme);
}
