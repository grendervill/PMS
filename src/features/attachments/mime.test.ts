import { describe, expect, it } from "vitest";
import { basename, formatBytes, mimeFromName } from "./mime";

describe("attachment mime helpers", () => {
  it("maps common extensions", () => {
    expect(mimeFromName("photo.PNG")).toBe("image/png");
    expect(mimeFromName("scan.pdf")).toBe("application/pdf");
    expect(mimeFromName("notes.md")).toBe("text/markdown");
    expect(mimeFromName("archive.zip")).toBe("application/octet-stream");
  });

  it("formats sizes", () => {
    expect(formatBytes(12)).toBe("12 Б");
    expect(formatBytes(2048)).toBe("2.0 КБ");
    expect(formatBytes(2 * 1024 * 1024)).toBe("2.0 МБ");
  });

  it("extracts a basename from unix and windows paths", () => {
    expect(basename("/tmp/photo.png")).toBe("photo.png");
    expect(basename("C:\\Users\\alex\\scan.pdf")).toBe("scan.pdf");
  });
});
