import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { vaultApi } from "@/features/vault/api";
import type { TotpConfig } from "@/features/vault/types";
import { TotpCode } from "./TotpCode";

vi.mock("@/features/vault/api", () => ({
  vaultApi: { totpCode: vi.fn() },
}));

const config: TotpConfig = {
  secret: "JBSWY3DPEHPK3PXP",
  algorithm: "SHA1",
  digits: 6,
  period: 30,
  issuer: "GitHub",
  account: "alex",
};

describe("TotpCode", () => {
  beforeEach(() => {
    vi.mocked(vaultApi.totpCode).mockReset();
    vi.mocked(vaultApi.totpCode).mockResolvedValue({ code: "123456", remaining: 25 });
  });

  it("renders a formatted live code", async () => {
    render(
      <ToastProvider>
        <TotpCode config={config} />
      </ToastProvider>,
    );

    expect(await screen.findByText("123 456")).toBeInTheDocument();
    expect(screen.getByText("25 с")).toBeInTheDocument();
    expect(vaultApi.totpCode).toHaveBeenCalledWith(config);
  });
});
