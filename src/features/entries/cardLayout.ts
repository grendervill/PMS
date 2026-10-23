export type CardLayout = "compact" | "list" | "advanced";

export type CardLayoutOption = {
  value: CardLayout;
  label: string;
  description: string;
};

export const CARD_LAYOUT_OPTIONS: readonly CardLayoutOption[] = [
  {
    value: "compact",
    label: "Компактно",
    description: "Только название и значок сайта.",
  },
  {
    value: "list",
    label: "Списком",
    description: "Название, значок и комментарий.",
  },
  {
    value: "advanced",
    label: "Продвинуто",
    description: "Название, URL, логин, пароль и комментарий.",
  },
];

export function resolveCardLayout(settings: { cardLayout?: CardLayout }): CardLayout {
  return settings.cardLayout ?? "list";
}
