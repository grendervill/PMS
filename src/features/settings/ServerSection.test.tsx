import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ServerSection } from "./ServerSection";

const profile = {
  id: "p-1",
  name: "pms.home",
  baseUrl: "https://pms.home:8443",
  accountId: "ab".repeat(16),
  tls: { kind: "pin" as const, spkiSha256: "cd".repeat(32) },
  lastLoginAt: "2026-11-19T10:00:00.000Z",
};

function renderSection(overrides: Partial<Parameters<typeof ServerSection>[0]> = {}) {
  return render(
    <ServerSection
      profile={profile}
      status={{ kind: "idle" }}
      usage={{ used: 250 * 1024 * 1024, quota: 1024 * 1024 * 1024, vaultBytes: 0, blobsBytes: 0, blobCount: 0 }}
      stale={false}
      onSyncNow={vi.fn()}
      onDisconnect={vi.fn()}
      onDeleteAccount={vi.fn()}
      {...overrides}
    />,
  );
}

describe("ServerSection", () => {
  it("shows used and free space with a proportional bar", () => {
    renderSection();

    expect(screen.getByText(/250 МиБ из 1\.0 ГиБ/)).toBeInTheDocument();
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "24");
  });

  it("never reports an impossible fill", () => {
    renderSection({
      usage: { used: 3 * 1024 * 1024 * 1024, quota: 1024 * 1024 * 1024, vaultBytes: 0, blobsBytes: 0, blobCount: 0 },
    });
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });

  it("marks the numbers as stale when offline", () => {
    renderSection({ status: { kind: "offline", message: "нет сети" }, stale: true });
    expect(screen.getByText(/устарело/i)).toBeInTheDocument();
    expect(screen.getByText(/нет сети/)).toBeInTheDocument();
  });

  it("explains a pending push", () => {
    renderSection({ status: { kind: "dirty" } });
    expect(screen.getByText(/есть неотправленное/i)).toBeInTheDocument();
  });
});
