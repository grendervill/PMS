import { beforeEach, describe, expect, it } from "vitest";
import { addRecent, lastRecent, listRecent, removeRecent } from "./recent";

const base = {
  name: "Личное",
  vaultId: "vault-1",
};

describe("recent vaults", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("stores and returns the most recent vault first", () => {
    addRecent({ ...base, path: "/a", lastOpenedAt: "2026-10-20T10:00:00Z" });
    addRecent({ ...base, path: "/b", lastOpenedAt: "2026-10-20T11:00:00Z" });

    expect(listRecent().map((item) => item.path)).toEqual(["/b", "/a"]);
    expect(lastRecent()?.path).toBe("/b");
  });

  it("updates an existing path without duplicating it", () => {
    addRecent({ ...base, path: "/a", lastOpenedAt: "2026-10-20T10:00:00Z" });
    addRecent({ ...base, path: "/a", lastOpenedAt: "2026-10-20T12:00:00Z" });

    expect(listRecent()).toHaveLength(1);
    expect(lastRecent()?.lastOpenedAt).toBe("2026-10-20T12:00:00Z");
  });

  it("removes a vault", () => {
    addRecent({ ...base, path: "/a", lastOpenedAt: "2026-10-20T10:00:00Z" });
    addRecent({ ...base, path: "/b", lastOpenedAt: "2026-10-20T11:00:00Z" });

    expect(removeRecent("/a").map((item) => item.path)).toEqual(["/b"]);
  });

  it("ignores malformed storage", () => {
    localStorage.setItem("pms.recentVaults.v1", "{broken");
    expect(listRecent()).toEqual([]);
  });
});
