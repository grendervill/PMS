import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import type { Entry } from "@/features/vault/types";
import { vaultApi } from "@/features/vault/api";
import { makeQrDataUrl } from "@/features/qr/qr";
import { ShareCardDialog } from "./ShareCardDialog";

vi.mock("@/features/qr/qr", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/features/qr/qr")>();
  return {
    ...actual,
    makeQrDataUrl: vi.fn(),
  };
});

const entry: Entry = {
  id: "entry-1",
  kind: "login",
  schemaVersion: 1,
  name: "GitHub",
  login: "alex",
  password: "s3cret",
  url: "https://github.com",
  urls: ["https://github.com", "https://gist.github.com"],
  comment: "личный",
  totp: null,
  attachmentIds: [],
  sshKeyIds: [],
  loginHistory: [],
  passwordHistory: [],
  createdAt: "2026-10-21T10:00:00.000Z",
  updatedAt: "2026-10-21T10:00:00.000Z",
};

function renderDialog() {
  return render(
    <ToastProvider>
      <ShareCardDialog entry={entry} onClose={vi.fn()} />
    </ToastProvider>,
  );
}

describe("ShareCardDialog", () => {
  beforeEach(() => {
    vi.mocked(makeQrDataUrl).mockResolvedValue("data:image/png;base64,AAAA");
    vi.spyOn(vaultApi, "shareEncode").mockResolvedValue("PMSQR1:payload");
  });

  afterEach(() => vi.restoreAllMocks());

  it("defaults to a passwordless QR and shows the warning", async () => {
    renderDialog();

    await screen.findByAltText("QR-код карточки");
    expect(screen.getByText(/данные карточки лежат в QR открыто/)).toBeInTheDocument();
    expect(vaultApi.shareEncode).toHaveBeenCalledWith(
      expect.objectContaining({ name: "GitHub", urls: entry.urls }),
      undefined,
    );
  });

  it("requires a password in secure mode", async () => {
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "С паролем" }));

    const create = screen.getByRole("button", { name: "Создать QR" });
    expect(create).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Пароль QR"), {
      target: { value: "qr-pass" },
    });
    await waitFor(() => expect(create).not.toBeDisabled());
    fireEvent.click(create);

    await waitFor(() =>
      expect(vaultApi.shareEncode).toHaveBeenCalledWith(expect.anything(), "qr-pass"),
    );
    await screen.findByAltText("QR-код карточки");
  });
});
