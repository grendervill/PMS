import { useEffect, useRef } from "react";

export function useAutoLock({
  enabled,
  onLock,
  timeoutMs = 5 * 60 * 1000,
}: {
  enabled: boolean;
  onLock: () => void;
  timeoutMs?: number;
}) {
  const onLockRef = useRef(onLock);

  useEffect(() => {
    onLockRef.current = onLock;
  }, [onLock]);

  useEffect(() => {
    if (!enabled) return;

    let timer: ReturnType<typeof setTimeout> | null = null;
    let hiddenAt: number | null = null;
    const reset = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        onLockRef.current();
      }, timeoutMs);
    };

    const onVisibilityChange = () => {
      if (document.hidden) {
        hiddenAt = Date.now();
        if (timer) clearTimeout(timer);
        return;
      }
      if (hiddenAt !== null && Date.now() - hiddenAt >= timeoutMs) {
        hiddenAt = null;
        onLockRef.current();
        return;
      }
      hiddenAt = null;
      reset();
    };

    reset();
    const events: Array<keyof WindowEventMap> = [
      "pointerdown",
      "keydown",
      "wheel",
      "touchstart",
      "mousemove",
    ];
    for (const event of events) window.addEventListener(event, reset, { passive: true });
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      if (timer) clearTimeout(timer);
      for (const event of events) window.removeEventListener(event, reset);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [enabled, timeoutMs]);
}
