import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAutoLock } from "./useAutoLock";

describe("useAutoLock", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("locks after the idle timeout", () => {
    const onLock = vi.fn();
    renderHook(() => useAutoLock({ enabled: true, onLock, timeoutMs: 1000 }));

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(onLock).toHaveBeenCalledTimes(1);
  });

  it("resets the timer on activity", () => {
    const onLock = vi.fn();
    renderHook(() => useAutoLock({ enabled: true, onLock, timeoutMs: 1000 }));

    act(() => {
      vi.advanceTimersByTime(500);
      window.dispatchEvent(new Event("pointerdown"));
      vi.advanceTimersByTime(600);
    });
    expect(onLock).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(onLock).toHaveBeenCalledTimes(1);
  });

  it("does nothing when disabled", () => {
    const onLock = vi.fn();
    renderHook(() => useAutoLock({ enabled: false, onLock, timeoutMs: 1000 }));

    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(onLock).not.toHaveBeenCalled();
  });
});
