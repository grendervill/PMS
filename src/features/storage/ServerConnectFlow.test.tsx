import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ServerConnectFlow } from "./ServerConnectFlow";

describe("ServerConnectFlow", () => {
  it("offers both ways and the existing server leads to the form", () => {
    render(<ServerConnectFlow onConnected={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText("У меня уже есть сервер")).toBeInTheDocument();
    expect(screen.getByText("Развернуть сервер")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Подключиться/ }));
    expect(screen.getByRole("heading", { name: "Подключение к серверу" })).toBeInTheDocument();
  });

  it("shows deployment steps and both certificate options", () => {
    render(<ServerConnectFlow onConnected={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Показать развёртывание/ }));

    expect(screen.getByRole("heading", { name: "Развёртывание сервера" })).toBeInTheDocument();
    expect(screen.getByText("Самоподписанный")).toBeInTheDocument();
    expect(screen.getByText("Let's Encrypt")).toBeInTheDocument();
    expect(screen.getByText(/openssl req -x509/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Let's Encrypt/ }));
    expect(screen.getByText(/reverse_proxy 127.0.0.1:8443/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Сервер готов/ }));
    expect(screen.getByRole("heading", { name: "Подключение к серверу" })).toBeInTheDocument();
  });

  it("skip on the setup screen goes straight to the form", () => {
    render(<ServerConnectFlow onConnected={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Показать развёртывание/ }));
    fireEvent.click(screen.getByRole("button", { name: /Пропустить настройку/ }));

    expect(screen.getByRole("heading", { name: "Подключение к серверу" })).toBeInTheDocument();
  });

  it("back from the form returns to the choice", () => {
    render(<ServerConnectFlow onConnected={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Подключиться/ }));
    fireEvent.click(screen.getByRole("button", { name: /Назад к выбору/ }));

    expect(screen.getByText("У меня уже есть сервер")).toBeInTheDocument();
  });
});
