import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { IconButton } from "@/components/ui/IconButton";
import { useToast } from "@/components/ui/toast-context";
import { copySecret } from "@/lib/clipboard";

export function CopyButton({
  value,
  label = "Копировать",
}: {
  value: string;
  label?: string;
}) {
  const [copied, setCopied] = useState(false);
  const toast = useToast();

  const copy = async () => {
    if (!value) return;
    try {
      await copySecret(value);
      setCopied(true);
      toast.show({
        title: "Скопировано",
        description: "Буфер обмена очистится через 30 секунд.",
      });
      setTimeout(() => setCopied(false), 1600);
    } catch {
      toast.show({
        title: "Не удалось скопировать",
        tone: "error",
      });
    }
  };

  return (
    <IconButton label={copied ? "Скопировано" : label} size="sm" onClick={() => void copy()}>
      {copied ? <Check size={14} className="text-[var(--accent)]" /> : <Copy size={14} />}
    </IconButton>
  );
}
