import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ServerProfileForm } from "./ServerProfileForm";
import { vaultApi } from "@/features/vault/api";

vi.mock("@/features/vault/api", () => ({
  vaultApi: {
    remoteProbe: vi.fn(),
    remoteLogin: vi.fn(),
    remoteRegister: vi.fn(),
    remoteCacheDir: vi.fn(),
  },
}));

const api = vi.mocked(vaultApi);

describe("ServerProfileForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("cannot log in before the server is checked", () => {
    render(<ServerProfileForm onConnected={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Войти/ })).toBeDisabled();
  });

  it("shows the fingerprint after a successful check", async () => {
    api.remoteProbe.mockResolvedValue({
      protocol: 1,
      spkiSha256: "ab".repeat(32),
    });
    render(<ServerProfileForm onConnected={vi.fn()} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText(/Адрес сервера/), {
      target: { value: "https://pms.home:8443" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Проверить сервер/ }));

    expect(await screen.findByText(/Отпечаток ключа/)).toBeInTheDocument();
    expect(screen.getByText(/не подтверждён публичным CA/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Войти/ })).toBeDisabled();

    fireEvent.change(screen.getByLabelText(/Логин на сервере/), {
      target: { value: "alice" },
    });
    fireEvent.change(screen.getByLabelText(/Пароль-фраза/), {
      target: { value: "correct horse" },
    });
    expect(screen.getByRole("button", { name: /Войти/ })).toBeEnabled();
  });

  it("reports an unreachable server as text, not a crash", async () => {
    api.remoteProbe.mockRejectedValue({
      code: "remote_unreachable",
      message: "сервер недоступен: connection refused",
    });
    render(<ServerProfileForm onConnected={vi.fn()} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText(/Адрес сервера/), {
      target: { value: "https://127.0.0.1:1" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Проверить сервер/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/connection refused/);
    expect(screen.getByRole("button", { name: /Войти/ })).toBeDisabled();
  });

  it("connects and hands over the saved profile", async () => {
    api.remoteProbe.mockResolvedValue({ protocol: 1, spkiSha256: "cd".repeat(32) });
    api.remoteLogin.mockResolvedValue({
      accountId: "ab".repeat(16),
      deviceId: "01".repeat(16),
      expiresAt: "2026-11-20T00:00:00Z",
      vault: null,
    });
    const onConnected = vi.fn();
    render(<ServerProfileForm onConnected={onConnected} onCancel={vi.fn()} />);

    fireEvent.change(screen.getByLabelText(/Адрес сервера/), {
      target: { value: "https://pms.home:8443" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Проверить сервер/ }));
    await screen.findByText(/Отпечаток ключа/);

    fireEvent.change(screen.getByLabelText(/Логин на сервере/), {
      target: { value: "alice" },
    });
    fireEvent.change(screen.getByLabelText(/Пароль-фраза/), {
      target: { value: "correct horse" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Войти/ }));

    await waitFor(() => expect(onConnected).toHaveBeenCalledOnce());
    const [profile] = onConnected.mock.calls[0] as unknown as [
      { baseUrl: string; accountId: string; login: string; tls: { kind: string } },
    ];
    expect(profile.baseUrl).toBe("https://pms.home:8443");
    expect(profile.login).toBe("alice");
    expect(profile.accountId).toBe("ab".repeat(16));
    expect(profile.tls.kind).toBe("pin");
  });
});
