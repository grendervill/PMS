import { render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { EntryFavicon } from "./EntryFavicon";
import { clearFaviconCache } from "./faviconCache";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const ICON = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

function renderIcon(host: string | null, enabled = true) {
  return render(<EntryFavicon name="GitHub" host={host} enabled={enabled} />);
}

describe("EntryFavicon", () => {
  beforeEach(() => {
    localStorage.clear();
    clearFaviconCache();
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows a monogram without a host and does not fetch", () => {
    const { container } = renderIcon(null);

    expect(container.textContent).toBe("G");
    expect(container.querySelector("img")).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("does not request an icon when loading is disabled", () => {
    const { container } = renderIcon("github.com", false);

    expect(container.textContent).toBe("G");
    expect(container.querySelector("img")).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("shows the fetched icon instead of the monogram", async () => {
    invokeMock.mockResolvedValue(ICON);
    const { container } = renderIcon("github.com");

    await waitFor(() => {
      expect(container.querySelector("img")?.getAttribute("src")).toBe(ICON);
    });
    expect(invokeMock).toHaveBeenCalledWith("favicon_fetch", { host: "github.com" });
  });

  it("reuses a cached icon when the card is mounted again", async () => {
    invokeMock.mockResolvedValue(ICON);
    const first = renderIcon("github.com");
    await waitFor(() => {
      expect(first.container.querySelector("img")?.getAttribute("src")).toBe(ICON);
    });
    first.unmount();

    const second = renderIcon("github.com");
    await waitFor(() => {
      expect(second.container.querySelector("img")?.getAttribute("src")).toBe(ICON);
    });
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("falls back to the monogram when the icon request fails", async () => {
    invokeMock.mockRejectedValue(new Error("network"));
    const { container } = renderIcon("github.com");

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    await waitFor(() => expect(container.textContent).toBe("G"));
    expect(container.querySelector("img")).toBeNull();
  });
});
