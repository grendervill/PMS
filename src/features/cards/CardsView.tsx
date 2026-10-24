import { CreditCard, Plus, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { useAttachmentUrl } from "@/features/attachments/useAttachmentUrl";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { BankCard } from "@/features/vault/types";
import { BankCardArt } from "./BankCardArt";
import { CardDetail } from "./CardDetail";
import { CardEditor } from "./CardEditor";
import { cardLabel } from "./cardModel";

function CardTile({
  card,
  onSelect,
}: {
  card: BankCard;
  onSelect: () => void;
}) {
  const imageUrl = useAttachmentUrl(card.imageAttachmentId);
  return (
    <button
      type="button"
      aria-label={cardLabel(card)}
      onClick={onSelect}
      className="block w-full rounded-[18px] text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)]"
    >
      <BankCardArt
        card={card}
        imageUrl={imageUrl}
        revealNumber={false}
        flipped={false}
      />
    </button>
  );
}

export function CardsView() {
  const data = useVaultStore((state) => state.data);
  const deleteBankCard = useVaultStore((state) => state.deleteBankCard);
  const confirm = useConfirm();
  const toast = useToast();
  const cards = useMemo(() => data?.bankCards ?? [], [data?.bankCards]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<BankCard | "new" | null>(null);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return cards;
    return cards.filter((card) =>
      [
        cardLabel(card),
        card.holder,
        card.purpose,
        card.country,
        card.address,
        card.number.slice(-4),
      ].some((value) => value.toLowerCase().includes(needle)),
    );
  }, [cards, query]);

  const selected = cards.find((card) => card.id === selectedId) ?? null;

  useEffect(() => {
    const onBack = (event: Event) => {
      if (editor) {
        setEditor(null);
        event.preventDefault();
        return;
      }
      if (selectedId) {
        setSelectedId(null);
        event.preventDefault();
      }
    };
    window.addEventListener("pms:android-back", onBack);
    return () => window.removeEventListener("pms:android-back", onBack);
  }, [editor, selectedId]);

  const remove = async (card: BankCard) => {
    const allowed = await confirm({
      title: `Удалить карту «${cardLabel(card)}»?`,
      description: "Это действие необратимо. Резервная копия хранилища останется.",
      confirmLabel: "Удалить",
      tone: "danger",
    });
    if (!allowed) return;

    const ok = await deleteBankCard(card.id);
    if (ok && selectedId === card.id) setSelectedId(null);
    if (!ok) {
      toast.show({
        title: "Не удалось удалить карту",
        description: errorText(useVaultStore.getState().error),
        tone: "error",
      });
    }
  };

  const onSaved = () => {
    if (editor !== "new") return;
    const created = useVaultStore.getState().data?.bankCards[0];
    if (created) setSelectedId(created.id);
  };

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="mx-auto w-full max-w-5xl">
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <Search
              size={14}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--muted)]"
            />
            <Input
              aria-label="Поиск по картам"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Поиск по владельцу, номеру или назначению"
              className="pl-9"
            />
          </div>
          <Button onClick={() => setEditor("new")}>
            <Plus size={15} />
            Добавить карту
          </Button>
        </div>

        {cards.length === 0 ? (
          <EmptyState
            icon={CreditCard}
            title="Пока нет карт"
            description="Добавьте банковскую карту, чтобы держать номер, срок и CVV под рукой."
            className="min-h-[320px]"
          />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={Search}
            title="Ничего не найдено"
            description="Попробуйте изменить запрос."
            className="min-h-[260px]"
          />
        ) : (
          <div className="grid gap-5 sm:grid-cols-2">
            {visible.map((card) => (
              <CardTile
                key={card.id}
                card={card}
                onSelect={() => setSelectedId(card.id)}
              />
            ))}
          </div>
        )}
      </div>

      {selected ? (
        <CardDetail
          key={selected.id}
          card={selected}
          onClose={() => setSelectedId(null)}
          onEdit={() => {
            setEditor(selected);
            setSelectedId(null);
          }}
          onDelete={() => void remove(selected)}
        />
      ) : null}

      {editor ? (
        <CardEditor
          card={editor === "new" ? null : editor}
          onClose={() => setEditor(null)}
          onSaved={onSaved}
        />
      ) : null}
    </div>
  );
}
