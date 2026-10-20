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
    const reset = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        onLockRef.current();
      }, timeoutMs);
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

    return () => {
      if (timer) clearTimeout(timer);
      for (const event of events) window.removeEventListener(event, reset);
    };
  }, [enabled, timeoutMs]);
}
