import { beforeEach, describe, expect, it } from "vitest";
import { addRecent } from "@/lib/storage/recent";
import { setMode } from "@/lib/storage/mode";
import { saveProfile, setActiveProfile } from "./profiles";
import { startupTarget } from "./startup";

const profile = {
  id: "p1",
  name: "Домашний",
  baseUrl: "https://pms.home:8443",
  accountId: "ab".repeat(16),
  tls: { kind: "pin" as const, spkiSha256: "cd".repeat(32) },
  lastLoginAt: "2026-11-19T10:00:00.000Z",
};

describe("startup target", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("asks for the mode on a fresh install even if a vault was opened before", () => {
    addRecent({ path: "/vaults/a", name: "A", vaultId: "1", lastOpenedAt: "2026-01-01" });
    expect(startupTarget()).toEqual({ kind: "choose" });
  });

  it("opens the last local vault in local mode", () => {
    setMode("local");
    addRecent({ path: "/vaults/a", name: "A", vaultId: "1", lastOpenedAt: "2026-01-01" });
    expect(startupTarget()).toEqual({ kind: "local", path: "/vaults/a" });
  });

  it("reports local mode without any vault", () => {
    setMode("local");
    expect(startupTarget()).toEqual({ kind: "local", path: null });
  });

  it("uses the active server profile in remote mode", () => {
    setMode("remote");
    saveProfile(profile);
    setActiveProfile(profile.id);
    expect(startupTarget()).toEqual({ kind: "remote", profile });
  });

  it("asks again when the remote profile is gone", () => {
    setMode("remote");
    expect(startupTarget()).toEqual({ kind: "choose" });
  });
});
