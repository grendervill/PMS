import * as RadixSwitch from "@radix-ui/react-switch";
import { cn } from "@/lib/cn";

export function Toggle({
  checked,
  onCheckedChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center justify-between gap-4 py-1.5",
        disabled && "cursor-not-allowed opacity-50",
      )}
    >
      <span className="min-w-0">
        <span className="block text-[12px] text-[var(--text-soft)]">{label}</span>
        {description ? (
          <span className="mt-0.5 block text-[10.5px] leading-relaxed text-[var(--muted)]">
            {description}
          </span>
        ) : null}
      </span>
      <RadixSwitch.Root
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className="relative h-5 w-9 shrink-0 rounded-full border border-[var(--border)] bg-[var(--surface-3)] outline-none transition-colors data-[state=checked]:border-[var(--accent)] data-[state=checked]:bg-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]/40 disabled:cursor-not-allowed"
      >
        <RadixSwitch.Thumb className="block size-3.5 translate-x-0.5 rounded-full bg-[var(--text-soft)] transition-transform data-[state=checked]:translate-x-[18px] data-[state=checked]:bg-[#08110d]" />
      </RadixSwitch.Root>
    </label>
  );
}
