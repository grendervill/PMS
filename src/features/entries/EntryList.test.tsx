import { fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EntryList } from "./EntryList";
import type { Entry } from "@/features/vault/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function makeEntries(count: number): Entry[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `entry-${index}`,
    kind: "login" as const,
    schemaVersion: 1,
    name: `Site ${index}`,
    login: `user-${index}`,
    password: "secret",
    url: "",
    urls: [],
    comment: "",
    totp: null,
    attachmentIds: [],
    sshKeyIds: [],
    loginHistory: [],
    passwordHistory: [],
    createdAt: "2026-10-20T10:00:00.000Z",
    updatedAt: "2026-10-20T10:00:00.000Z",
  }));
}

type ListProps = Parameters<typeof EntryList>[0];

function listProps(entries: Entry[], overrides: Partial<ListProps> = {}): ListProps {
  return {
    entries,
    total: entries.length,
    selectedId: null,
    query: "",
    sort: "name",
    onQuery: () => undefined,
    onSort: () => undefined,
    onSelect: () => undefined,
    onCreate: () => undefined,
    onScanQr: () => undefined,
    layout: "list",
    faviconEnabled: false,
    ...overrides,
  };
}

function scroller(container: HTMLElement): HTMLElement {
  const element = container.querySelector<HTMLElement>(".pms-scrollbar");
  if (!element) throw new Error("scroll container is missing");
  return element;
}

/** jsdom has no layout: give the container a viewport and a scroll offset. */
function setViewport(element: HTMLElement, height: number, scrollTop = 0): void {
  Object.defineProperty(element, "clientHeight", { value: height, configurable: true });
  Object.defineProperty(element, "scrollTop", {
    value: scrollTop,
    writable: true,
    configurable: true,
  });
}

function cards(container: HTMLElement): number {
  return container.querySelectorAll("[data-entry-id]").length;
}

afterEach(() => {
  delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
});

describe("EntryList", () => {
  it("renders every card for a short list", () => {
    const entries = makeEntries(50);
    const { container } = render(<EntryList {...listProps(entries)} />);

    expect(cards(container)).toBe(50);
    expect(container.querySelectorAll("[data-entry-row]")).toHaveLength(0);
  });

  it("mounts only a window of cards for a long list", () => {
    const entries = makeEntries(500);
    const { container } = render(<EntryList {...listProps(entries)} />);
    const list = scroller(container);
    setViewport(list, 600);
    fireEvent.scroll(list);

    const mounted = cards(container);
    expect(mounted).toBeGreaterThan(5);
    expect(mounted).toBeLessThan(60);
    expect(container.querySelectorAll("[data-entry-row]")).toHaveLength(mounted);
    expect(container.querySelector("[data-entry-id='entry-0']")).not.toBeNull();
  });

  it("keeps the window small at 5000 entries and moves it while scrolling", () => {
    const entries = makeEntries(5000);
    const { container } = render(<EntryList {...listProps(entries)} />);
    const list = scroller(container);
    setViewport(list, 600);
    fireEvent.scroll(list);
    expect(cards(container)).toBeLessThan(60);

    setViewport(list, 600, 3000);
    fireEvent.scroll(list);

    expect(cards(container)).toBeLessThan(60);
    expect(container.querySelector("[data-entry-id='entry-0']")).toBeNull();
    const topPad = container.querySelector<HTMLElement>("[data-entry-pad='top']");
    expect(Number(topPad?.style.height.replace("px", ""))).toBeGreaterThan(0);
  });

  it("resets the window when the search changes", () => {
    const entries = makeEntries(500);
    const { container, rerender } = render(<EntryList {...listProps(entries)} />);
    const list = scroller(container);
    setViewport(list, 600, 4000);
    fireEvent.scroll(list);
    expect(list.scrollTop).toBe(4000);

    rerender(<EntryList {...listProps(entries)} query="site 1" />);

    expect(list.scrollTop).toBe(0);
    expect(container.querySelector("[data-entry-id='entry-0']")).not.toBeNull();
  });

  it("scrolls the selected entry into view", async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      value: scrollIntoView,
      configurable: true,
      writable: true,
    });

    const entries = makeEntries(500);
    const props = listProps(entries);
    const { container, rerender } = render(<EntryList {...props} />);
    const list = scroller(container);
    setViewport(list, 600);
    fireEvent.scroll(list);

    rerender(<EntryList {...props} selectedId={entries[300]!.id} />);

    await waitFor(() => expect(scrollIntoView).toHaveBeenCalled());
    expect(list.scrollTop).toBeGreaterThan(0);
    expect(container.querySelector("[data-entry-id='entry-300']")).not.toBeNull();
  });
});
