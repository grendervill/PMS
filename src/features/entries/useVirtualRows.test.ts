import { describe, expect, it } from "vitest";
import { computeVirtualRange } from "./useVirtualRows";

describe("computeVirtualRange", () => {
  it("renders the visible rows plus overscan on both sides", () => {
    const range = computeVirtualRange({
      count: 5000,
      scrollTop: 60 * 100,
      viewportHeight: 600,
      rowHeight: 60,
    });

    expect(range.start).toBe(94);
    expect(range.end).toBe(117);
    expect(range.topPad).toBe(94 * 60);
    expect(range.bottomPad).toBe((5000 - 117) * 60);
  });

  it("keeps the window inside the list bounds at the edges", () => {
    const top = computeVirtualRange({ count: 20, scrollTop: 0, viewportHeight: 600, rowHeight: 60 });
    expect(top.start).toBe(0);
    expect(top.end).toBe(17);
    expect(top.topPad).toBe(0);
    expect(top.bottomPad).toBe(3 * 60);

    const bottom = computeVirtualRange({
      count: 5000,
      scrollTop: 5000 * 60,
      viewportHeight: 600,
      rowHeight: 60,
    });
    expect(bottom.end).toBe(5000);
    expect(bottom.bottomPad).toBe(0);
  });

  it("does not depend on a positive viewport height", () => {
    const range = computeVirtualRange({ count: 500, scrollTop: 0, viewportHeight: 0, rowHeight: 60 });
    expect(range.start).toBe(0);
    expect(range.end).toBe(7);
  });
});
