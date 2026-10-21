import { describe, expect, it } from "vitest";
import { previewMime, safeDownloadName, safePreviewKind } from "./preview";
import type { AttachmentMeta } from "@/features/vault/types";

function meta(overrides: Partial<AttachmentMeta>): AttachmentMeta {
  return {
    id: "a",
    name: "file",
    mime: "text/plain",
    size: 1,
    sha256: "hash",
    kind: "text",
    createdAt: "2026-10-20T10:00:00.000Z",
    ...overrides,
  };
}

describe("attachment preview safety", () => {
  it("rejects kind/mime mismatches", () => {
    expect(safePreviewKind(meta({ kind: "pdf", mime: "text/html" }))).toBe("other");
    expect(safePreviewKind(meta({ kind: "image", mime: "image/svg+xml" }))).toBe("other");
    expect(safePreviewKind(meta({ kind: "text", mime: "application/pdf" }))).toBe("other");
  });

  it("forces safe MIME types", () => {
    expect(previewMime(meta({ kind: "pdf", mime: "application/pdf" }), "pdf")).toBe(
      "application/pdf",
    );
    expect(previewMime(meta({ kind: "image", mime: "image/png" }), "image")).toBe(
      "image/png",
    );
    expect(previewMime(meta({ kind: "image", mime: "text/html" }), "other")).toBe(
      "application/octet-stream",
    );
  });

  it("strips path separators from download names", () => {
    expect(safeDownloadName("../../secret.txt")).toBe("secret.txt");
    expect(safeDownloadName("dir\\file.txt")).toBe("file.txt");
    expect(safeDownloadName("")).toBe("attachment");
  });
});
