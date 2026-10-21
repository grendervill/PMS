import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import type { Entry } from "@/features/vault/types";
import { EntryDetail } from "./EntryDetail";

const entry: Entry = {
  id: "entry-1",
  kind: "login",
  schemaVersion: 1,
  name: "GitHub",
  login: "alex",
  password: "s3cret",
  url: "https://github.com",
  urls: ["https://github.com", "https://gist.github.com"],
  comment: "",
  totp: null,
  attachmentIds: [],
  sshKeyIds: [],
  loginHistory: [],
  passwordHistory: [],
  createdAt: "2026-10-20T10:00:00.000Z",
  updatedAt: "2026-10-20T10:00:00.000Z",
};

function renderDetail() {
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <EntryDetail
          entry={entry}
          onBack={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          onHistory={vi.fn()}
        />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

describe("EntryDetail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reveals a password and hides it again after 30 seconds", () => {
    renderDetail();

    fireEvent.click(screen.getByRole("button", { name: "Показать пароль" }));
    expect(screen.getByText("s3cret")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(30_000);
    });

    expect(screen.queryByText("s3cret")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Показать пароль" })).toBeInTheDocument();
  });

  it("shows every url in the entry", () => {
    renderDetail();

    expect(screen.getByText("https://github.com")).toBeInTheDocument();
    expect(screen.getByText("https://gist.github.com")).toBeInTheDocument();
  });
});
