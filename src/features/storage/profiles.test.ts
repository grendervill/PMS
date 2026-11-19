import { beforeEach, describe, expect, it } from "vitest";
import {
  activeProfile,
  listProfiles,
  removeProfile,
  saveProfile,
  setActiveProfile,
  type ServerProfile,
} from "./profiles";

function profile(overrides: Partial<ServerProfile> = {}): ServerProfile {
  return {
    id: "p1",
    name: "Домашний сервер",
    baseUrl: "https://pms.home:8443",
    accountId: "ab".repeat(16),
    tls: { kind: "pin", spkiSha256: "cd".repeat(32) },
    lastLoginAt: "2026-11-19T10:00:00.000Z",
    ...overrides,
  };
}

describe("server profiles", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("saves, lists and updates by id", () => {
    saveProfile(profile());
    saveProfile(profile({ id: "p2", name: "Работа" }));
    expect(listProfiles().map((item) => item.id)).toEqual(["p2", "p1"]);

    saveProfile(profile({ name: "Домашний" }));
    expect(listProfiles()).toHaveLength(2);
    expect(listProfiles().find((item) => item.id === "p1")?.name).toBe("Домашний");
  });

  it("keeps the most recent profile first", () => {
    saveProfile(profile({ id: "old", lastLoginAt: "2026-01-01T00:00:00.000Z" }));
    saveProfile(profile({ id: "new", lastLoginAt: "2026-11-19T00:00:00.000Z" }));
    expect(listProfiles()[0]?.id).toBe("new");
  });

  it("resets the active profile when it is removed", () => {
    saveProfile(profile());
    setActiveProfile("p1");
    expect(activeProfile()?.id).toBe("p1");

    removeProfile("p1");
    expect(activeProfile()).toBeNull();
    expect(listProfiles()).toHaveLength(0);
  });

  it("drops malformed entries instead of throwing", () => {
    localStorage.setItem("pms.profiles.v1", "не json");
    expect(listProfiles()).toEqual([]);

    localStorage.setItem(
      "pms.profiles.v1",
      JSON.stringify([{ id: "x" }, profile({ id: "ok" })]),
    );
    expect(listProfiles().map((item) => item.id)).toEqual(["ok"]);
  });
});
