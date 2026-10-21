import type { AttachmentMeta } from "@/features/vault/types";

export type PreviewKind = "image" | "pdf" | "text" | "other";

export function safePreviewKind(meta: AttachmentMeta): PreviewKind {
  const mime = meta.mime.toLowerCase();
  if (meta.kind === "image" && mime.startsWith("image/") && !mime.includes("svg")) {
    return "image";
  }
  if (meta.kind === "pdf" && mime === "application/pdf") {
    return "pdf";
  }
  if (meta.kind === "text" && mime.startsWith("text/")) {
    return "text";
  }
  return "other";
}

export function previewMime(meta: AttachmentMeta, kind: PreviewKind): string {
  if (kind === "pdf") return "application/pdf";
  if (kind === "text") return "text/plain";
  if (kind === "image") {
    const mime = meta.mime.toLowerCase();
    return mime.startsWith("image/") && !mime.includes("svg")
      ? mime
      : "application/octet-stream";
  }
  return "application/octet-stream";
}

export function safeDownloadName(name: string): string {
  const base = name.split(/[\\/]/).filter(Boolean).at(-1) ?? "attachment";
  const cleaned = base.replace(/^\.+/, "").trim();
  return cleaned || "attachment";
}
