import type {
  FavoriteKind,
  FavoriteRef,
  VaultData,
} from "@/features/vault/types";

export const FAVORITE_SECTIONS: readonly {
  kind: FavoriteKind;
  title: string;
}[] = [
  { kind: "entry", title: "Записи" },
  { kind: "totp", title: "TOTP" },
  { kind: "ssh", title: "SSH-ключи" },
  { kind: "card", title: "Карты" },
  { kind: "document", title: "Документы" },
];

export function favoritesOf(
  data: Pick<VaultData, "favorites"> | null | undefined,
): FavoriteRef[] {
  return data?.favorites ?? [];
}

export function isFavorite(
  data: Pick<VaultData, "favorites"> | null | undefined,
  kind: FavoriteKind,
  id: string,
): boolean {
  return favoritesOf(data).some((item) => item.kind === kind && item.id === id);
}

export function withoutFavorite(
  favorites: FavoriteRef[],
  kind: FavoriteKind,
  id: string,
): FavoriteRef[] {
  return favorites.filter((item) => !(item.kind === kind && item.id === id));
}

export function withoutItemFavorites(
  favorites: FavoriteRef[],
  kinds: readonly FavoriteKind[],
  id: string,
): FavoriteRef[] {
  return favorites.filter(
    (item) => !(item.id === id && kinds.includes(item.kind)),
  );
}
