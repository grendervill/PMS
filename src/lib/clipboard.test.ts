import { beforeEach, describe, expect, it, vi } from "vitest";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { copySecret } from "./clipboard";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
  readText: vi.fn(),
}));

const writeMock = vi.mocked(writeText);
const readMock = vi.mocked(readText);

describe("copySecret", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    writeMock.mockReset();
    readMock.mockReset();
  });

  it("clears the clipboard after the timeout if the value is unchanged", async () => {
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("secret");

    await copySecret("secret", { clearAfterMs: 1000 });
    expect(writeMock).toHaveBeenCalledWith("secret");

    await vi.advanceTimersByTimeAsync(1000);

    expect(readMock).toHaveBeenCalled();
    expect(writeMock).toHaveBeenLastCalledWith("");
    vi.useRealTimers();
  });

  it("does not clear a newer clipboard value", async () => {
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("something else");

    await copySecret("secret", { clearAfterMs: 1000 });
    await vi.advanceTimersByTimeAsync(1000);

    expect(writeMock).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it("cancels a previous timer when copying again", async () => {
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("first");

    await copySecret("first", { clearAfterMs: 1000 });
    await vi.advanceTimersByTimeAsync(500);
    await copySecret("second", { clearAfterMs: 1000 });
    await vi.advanceTimersByTimeAsync(600);

    expect(readMock).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("keeps the clipboard when auto-clear is disabled", async () => {
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("secret");

    await copySecret("secret", { clearAfterMs: 0 });
    await vi.advanceTimersByTimeAsync(60_000);

    expect(readMock).not.toHaveBeenCalled();
    expect(writeMock).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it("cancels a pending clear when a later copy disables auto-clear", async () => {
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("first");

    await copySecret("first", { clearAfterMs: 1000 });
    await vi.advanceTimersByTimeAsync(500);
    await copySecret("second", { clearAfterMs: 0 });
    await vi.advanceTimersByTimeAsync(1000);

    expect(readMock).not.toHaveBeenCalled();
    expect(writeMock).toHaveBeenLastCalledWith("second");
    vi.useRealTimers();
  });
});
