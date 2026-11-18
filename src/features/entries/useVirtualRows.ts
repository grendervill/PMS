import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

/** Lists above this length are rendered through a window instead of all at once. */
export const VIRTUAL_LIST_THRESHOLD = 150;
/** Extra rows rendered above and below the viewport. */
export const VIRTUAL_LIST_OVERSCAN = 6;

export type VirtualRange = {
  start: number;
  end: number;
  topPad: number;
  bottomPad: number;
};

export function computeVirtualRange({
  count,
  scrollTop,
  viewportHeight,
  rowHeight,
  overscan = VIRTUAL_LIST_OVERSCAN,
}: {
  count: number;
  scrollTop: number;
  viewportHeight: number;
  rowHeight: number;
  overscan?: number;
}): VirtualRange {
  const height = Math.max(1, rowHeight);
  const safeScroll = Math.max(0, scrollTop);
  const first = Math.min(Math.max(0, Math.floor(safeScroll / height)), Math.max(0, count - 1));
  const visible = Math.ceil(Math.max(0, viewportHeight) / height) + 1;
  const start = Math.max(0, first - overscan);
  const end = Math.min(count, first + visible + overscan);
  return {
    start,
    end,
    topPad: start * height,
    bottomPad: Math.max(0, (count - end) * height),
  };
}

/**
 * Windowing for a single-column list inside its own scroll container.
 * Below the threshold nothing changes: the caller renders the full list.
 */
export function useVirtualRows({
  count,
  enabled,
  estimatedRowHeight,
  resetKey,
}: {
  count: number;
  enabled: boolean;
  estimatedRowHeight: number;
  resetKey?: unknown;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const rowRef = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 0 });
  const [measurement, setMeasurement] = useState({ estimate: estimatedRowHeight, height: 0 });

  // A measurement belongs to the layout it was taken in; a new layout falls
  // back to the estimate until the first row is measured again.
  const rowHeight =
    measurement.estimate === estimatedRowHeight && measurement.height > 0
      ? measurement.height
      : estimatedRowHeight;

  const syncViewport = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    const next = { top: container.scrollTop, height: container.clientHeight };
    setViewport((current) =>
      current.top === next.top && current.height === next.height ? current : next,
    );
  }, []);

  useLayoutEffect(() => {
    if (!enabled) return;
    const container = containerRef.current;
    if (!container) return;
    syncViewport();

    const onScroll = () => syncViewport();
    container.addEventListener("scroll", onScroll, { passive: true });

    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver === "function") {
      observer = new ResizeObserver(() => syncViewport());
      observer.observe(container);
    }

    return () => {
      container.removeEventListener("scroll", onScroll);
      observer?.disconnect();
    };
  }, [enabled, syncViewport]);

  // The first rendered row gives the real height; the estimate is only a seed.
  useEffect(() => {
    if (!enabled) return;
    const row = rowRef.current;
    if (!row) return;

    const syncRow = () => {
      const height = row.getBoundingClientRect().height;
      if (height <= 0) return;
      setMeasurement((current) =>
        current.estimate === estimatedRowHeight && Math.abs(current.height - height) < 1
          ? current
          : { estimate: estimatedRowHeight, height },
      );
    };

    syncRow();
    if (typeof ResizeObserver !== "function") return;
    const observer = new ResizeObserver(syncRow);
    observer.observe(row);
    return () => observer.disconnect();
  }, [enabled, estimatedRowHeight]);

  // A new query or sort order starts at the top again.
  useLayoutEffect(() => {
    if (!enabled) return;
    const container = containerRef.current;
    if (!container) return;
    container.scrollTop = 0;
    syncViewport();
  }, [enabled, resetKey, syncViewport]);

  const range = useMemo(
    () =>
      enabled
        ? computeVirtualRange({
            count,
            scrollTop: viewport.top,
            viewportHeight: viewport.height,
            rowHeight,
          })
        : { start: 0, end: count, topPad: 0, bottomPad: 0 },
    [count, enabled, rowHeight, viewport.top, viewport.height],
  );

  /** Scrolls far enough for `index` to be inside the rendered window. */
  const scrollToRow = useCallback(
    (index: number) => {
      const container = containerRef.current;
      if (!enabled || !container || index < 0) return;
      const height = rowHeight > 0 ? rowHeight : estimatedRowHeight;
      const top = index * height;
      const bottom = top + height;
      const visibleTop = container.scrollTop;
      const visibleBottom = visibleTop + container.clientHeight;
      if (top >= visibleTop && bottom <= visibleBottom) return;
      container.scrollTop = Math.max(0, top - height * 2);
      syncViewport();
    },
    [enabled, estimatedRowHeight, rowHeight, syncViewport],
  );

  return {
    virtual: enabled,
    containerRef,
    rowRef,
    range,
    scrollToRow,
  };
}
