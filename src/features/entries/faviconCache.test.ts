import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  FAVICON_CACHE_LIMIT,
  FAVICON_CACHE_TTL_MS,
  FAVICON_MISS_TTL_MS,
  FAVICON_STORAGE_KEY,
  clearFaviconCache,
  loadFavicon,
  normalizeFaviconHost,
} from "./faviconCache";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const ICON = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

function storedHosts(): Record<string, unknown> {
  const raw = localStorage.getItem(FAVICON_STORAGE_KEY);
  if (!raw) return {};
  const parsed = JSON.parse(raw) as { hosts?: Record<string, unknown> };
  return parsed.hosts ?? {};
}

describe("favicon cache", () => {
  beforeEach(() => {
    localStorage.clear();
    clearFaviconCache();
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(ICON);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reuses a cached icon for every card with the same host", async () => {
    expect(await loadFavicon("github.com")).toBe(ICON);
    expect(await loadFavicon("github.com")).toBe(ICON);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("favicon_fetch", { host: "github.com" });
  });

  it("shares one request between cards mounted at the same time", async () => {
    let resolveRequest: (value: string) => void = () => undefined;
    invokeMock.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveRequest = resolve;
      }),
    );

    const first = loadFavicon("github.com");
    const second = loadFavicon("github.com");

    expect(invokeMock).toHaveBeenCalledTimes(1);
    resolveRequest(ICON);
    await expect(first).resolves.toBe(ICON);
    await expect(second).resolves.toBe(ICON);
  });

  it("normalizes case, www and the trailing dot before caching", async () => {
    expect(await loadFavicon("WWW.GitHub.com.")).toBe(ICON);
    expect(await loadFavicon("github.com")).toBe(ICON);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("favicon_fetch", { host: "github.com" });
  });

  it("ignores values that are not hostnames", async () => {
    await expect(loadFavicon("https://github.com/path")).resolves.toBeNull();
    await expect(loadFavicon("")).resolves.toBeNull();
    await expect(loadFavicon("localhost")).resolves.toBeNull();

    expect(invokeMock).not.toHaveBeenCalled();
    expect(normalizeFaviconHost("sub.example.co.uk")).toBe("sub.example.co.uk");
  });

  it("downloads an icon again after the cache expires", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-23T10:00:00.000Z"));
    await loadFavicon("github.com");

    vi.setSystemTime(new Date(Date.now() + FAVICON_CACHE_TTL_MS + 1000));
    await loadFavicon("github.com");

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("remembers a failed lookup briefly and retries after an hour", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-23T10:00:00.000Z"));
    invokeMock.mockRejectedValue(new Error("network"));

    await expect(loadFavicon("missing.example")).resolves.toBeNull();
    await expect(loadFavicon("missing.example")).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date(Date.now() + FAVICON_MISS_TTL_MS + 1000));
    await expect(loadFavicon("missing.example")).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("keeps icons between app launches", async () => {
    await loadFavicon("github.com");
    vi.resetModules();
    invokeMock.mockRejectedValue(new Error("must not fetch"));

    const fresh = await import("./faviconCache");
    await expect(fresh.loadFavicon("github.com")).resolves.toBe(ICON);
  });

  it("limits the number of stored hosts", async () => {
    for (let index = 0; index <= FAVICON_CACHE_LIMIT; index += 1) {
      await loadFavicon(`site-${index}.example`);
    }

    expect(Object.keys(storedHosts())).toHaveLength(FAVICON_CACHE_LIMIT);
    expect(storedHosts()["site-0.example"]).toBeUndefined();
    expect(storedHosts()[`site-${FAVICON_CACHE_LIMIT}.example`]).toBeDefined();
  });
});
