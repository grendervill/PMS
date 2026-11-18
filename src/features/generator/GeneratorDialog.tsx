import { Modal } from "@/components/ui/Modal";
import { GeneratorPanel } from "@/features/generator/GeneratorPanel";
import type { PasswordPolicy } from "@/features/vault/types";

export function GeneratorDialog({
  mode,
  open,
  onOpenChange,
  onUse,
  policy,
}: {
  mode: "password" | "login";
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUse?: (value: string) => void;
  /** Site rules that preconfigure the password generator. */
  policy?: PasswordPolicy | null;
}) {
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={mode === "password" ? "Генератор пароля" : "Генератор логина"}
      description={
        mode === "password" && policy
          ? "Настройки подставлены по правилам сайта — их можно ужесточить."
          : "Настройте правила и пересоздавайте, пока результат не устроит."
      }
      className="w-[min(92vw,520px)]"
    >
      <GeneratorPanel mode={mode} onUse={onUse} policy={policy} />
    </Modal>
  );
}
