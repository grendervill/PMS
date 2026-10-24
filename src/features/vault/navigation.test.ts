import { describe, expect, it } from "vitest";
import { navItems } from "./navigation";

describe("navigation", () => {
  it("places cards and documents after entries", () => {
    expect(navItems.map((item) => item.id)).toEqual([
      "entries",
      "cards",
      "documents",
      "totp",
      "ssh",
      "transfer",
      "generator",
      "audit",
      "settings",
    ]);
  });
});
