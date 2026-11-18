import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Entry } from "@/features/vault/types";
import { EntryCard } from "./EntryCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const NOW = new Date();

function makeEntry(overrides: Partial<Entry> = {}): Entry {
  return {
    id: "entry-1",
    kind: "login",
    schemaVersion: 1,
    name: "GitHub",
    login: "alex",
    password: "s3cret",
    url: "https://github.com",
    urls: ["https://github.com"],
    comment: "Рабочий аккаунт",
    totp: null,
    attachmentIds: [],
    sshKeyIds: [],
    loginHistory: [],
    passwordHistory: [],
    createdAt: "2026-10-20T10:00:00.000Z",
    updatedAt: NOW.toISOString(),
    ...overrides,
  };
}

const oldDate = new Date(NOW.getTime() - 214 * 86_400_000).toISOString();

const policy = {
  minLength: 12,
  maxLength: 20,
  requireLowercase: true,
  requireUppercase: false,
  requireDigits: true,
  requireSymbols: false,
  forbiddenChars: "<>",
};

function renderCard(entry: Entry, layout: "list" | "compact" | "advanced" = "list") {
  return render(
    <EntryCard
      entry={entry}
      active={false}
      layout={layout}
      faviconEnabled={false}
      onSelect={vi.fn()}
    />,
  );
}

describe("EntryCard marks", () => {
  it("renders a plain card when the entry has no policy and a fresh password", () => {
    renderCard(makeEntry());

    expect(screen.getByText("GitHub")).toBeInTheDocument();
    expect(screen.queryByLabelText("Заданы правила пароля")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Пароль старше 180 дней")).not.toBeInTheDocument();
  });

  it("shows the policy mark when the site has password rules", () => {
    renderCard(makeEntry({ policy }));

    expect(screen.getByRole("img", { name: "Заданы правила пароля" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Пароль старше 180 дней")).not.toBeInTheDocument();
  });

  it("marks a password older than 180 days and not one that is younger", () => {
    const { unmount } = renderCard(makeEntry({ updatedAt: oldDate }));
    expect(screen.getByRole("img", { name: "Пароль старше 180 дней" })).toBeInTheDocument();
    unmount();

    renderCard(makeEntry({ updatedAt: new Date(NOW.getTime() - 90 * 86_400_000).toISOString() }));
    expect(screen.queryByLabelText("Пароль старше 180 дней")).not.toBeInTheDocument();
  });

  it("shows both marks in every layout without dropping the content", () => {
    for (const layout of ["list", "compact", "advanced"] as const) {
      const { unmount } = renderCard(makeEntry({ policy, updatedAt: oldDate }), layout);
      expect(screen.getByLabelText("Заданы правила пароля")).toBeInTheDocument();
      expect(screen.getByLabelText("Пароль старше 180 дней")).toBeInTheDocument();
      expect(screen.getByText("GitHub")).toBeInTheDocument();
      unmount();
    }
  });

  it("does not flag an entry that has no password at all", () => {
    renderCard(makeEntry({ password: "", updatedAt: oldDate }));
    expect(screen.queryByLabelText("Пароль старше 180 дней")).not.toBeInTheDocument();
  });
});
