import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { IconButton } from "@/components/ui/IconButton";
import {
  ToastContext,
  type ToastInput,
  type ToastItem,
  type ToastTone,
} from "@/components/ui/toast-context";
import { cn } from "@/lib/cn";

const toneIcon = {
  info: Info,
  success: CheckCircle2,
  error: AlertTriangle,
} satisfies Record<ToastTone, typeof Info>;

const toneClass: Record<ToastTone, string> = {
  info: "text-[var(--accent-soft)]",
  success: "text-[var(--accent)]",
  error: "text-[var(--danger)]",
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  const dismiss = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
    const timer = timers.current.get(id);
    if (timer) clearTimeout(timer);
    timers.current.delete(id);
  }, []);

  const show = useCallback(
    (input: ToastInput) => {
      const id = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
      setToasts((current) => [...current.slice(-2), { ...input, id }]);
      const timer = setTimeout(() => dismiss(id), input.durationMs ?? 4200);
      timers.current.set(id, timer);
      return id;
    },
    [dismiss],
  );

  const value = useMemo(() => ({ show, dismiss }), [show, dismiss]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        aria-live="polite"
        className="pointer-events-none fixed inset-x-4 bottom-20 z-[70] flex flex-col items-end gap-2 sm:bottom-5 sm:left-auto sm:right-5 sm:w-[min(92vw,380px)]"
      >
        <AnimatePresence initial={false}>
          {toasts.map((toast) => {
            const tone = toast.tone ?? "info";
            const Icon = toneIcon[tone];
            return (
              <motion.div
                key={toast.id}
                initial={{ opacity: 0, y: 8, scale: 0.98 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: 6, scale: 0.98 }}
                transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                role={tone === "error" ? "alert" : "status"}
                className="pointer-events-auto flex w-full items-start gap-3 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)] px-3.5 py-3 shadow-[var(--shadow-float)]"
              >
                <Icon size={18} className={cn("mt-0.5 shrink-0", toneClass[tone])} />
                <div className="min-w-0 flex-1">
                  <p className="text-[13px] font-medium text-[var(--text)]">{toast.title}</p>
                  {toast.description ? (
                    <p className="mt-0.5 text-xs leading-relaxed text-[var(--muted)]">
                      {toast.description}
                    </p>
                  ) : null}
                </div>
                <IconButton
                  label="Закрыть уведомление"
                  size="sm"
                  className="-mr-1 -mt-1"
                  onClick={() => dismiss(toast.id)}
                >
                  <X size={14} />
                </IconButton>
              </motion.div>
            );
          })}
        </AnimatePresence>
      </div>
    </ToastContext.Provider>
  );
}
