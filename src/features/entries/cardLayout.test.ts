import { describe, expect, it } from "vitest";
import { CARD_LAYOUT_OPTIONS, resolveCardLayout } from "./cardLayout";

describe("card layout options", () => {
  it("offers compact, standard and advanced layouts", () => {
    expect(CARD_LAYOUT_OPTIONS.map((option) => option.value)).toEqual([
      "compact",
      "list",
      "advanced",
    ]);
  });

  it("defaults old vaults to the standard list", () => {
    expect(resolveCardLayout({})).toBe("list");
    expect(resolveCardLayout({ cardLayout: "advanced" })).toBe("advanced");
  });
});
