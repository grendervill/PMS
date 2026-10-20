import { Modal } from "@/components/ui/Modal";
import { GeneratorPanel } from "@/features/generator/GeneratorPanel";

export function GeneratorDialog({
  mode,
  open,
  onOpenChange,
  onUse,
}: {
  mode: "password" | "login";
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUse?: (value: string) => void;
}) {
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={mode === "password" ? "Генератор пароля" : "Генератор логина"}
      description="Настройте правила и пересоздавайте, пока результат не устроит."
      className="w-[min(92vw,520px)]"
    >
      <GeneratorPanel mode={mode} onUse={onUse} />
    </Modal>
  );
}
