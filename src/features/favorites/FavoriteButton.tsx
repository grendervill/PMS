import { Star } from "lucide-react";
import { useState } from "react";
import { IconButton } from "@/components/ui/IconButton";
import { useToast } from "@/components/ui/toast-context";
import { errorText } from "@/features/vault/errors";
import { isFavorite } from "@/features/favorites/favorites";
import { useVaultStore } from "@/features/vault/store";
import type { FavoriteKind } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function FavoriteButton({
  kind,
  id,
  label = "Добавить в избранное",
  activeLabel = "Убрать из избранного",
  className,
}: {
  kind: FavoriteKind;
  id: string;
  label?: string;
  activeLabel?: string;
  className?: string;
}) {
  const data = useVaultStore((state) => state.data);
  const toggleFavorite = useVaultStore((state) => state.toggleFavorite);
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const active = isFavorite(data, kind, id);

  const toggle = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const ok = await toggleFavorite(kind, id);
      if (!ok) {
        toast.show({
          title: "Не удалось изменить избранное",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
        return;
      }
      toast.show({
        title: active ? "Убрано из избранного" : "Добавлено в избранное",
        tone: "success",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <IconButton
      label={active ? activeLabel : label}
      size="sm"
      aria-pressed={active}
      disabled={busy}
      onClick={() => void toggle()}
      className={cn(
        active && "text-[var(--seal)] hover:text-[var(--seal)]",
        className,
      )}
    >
      <Star
        size={15}
        strokeWidth={1.9}
        fill={active ? "currentColor" : "none"}
      />
    </IconButton>
  );
}
