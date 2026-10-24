import { useEffect, useState } from "react";
import { useVaultStore } from "@/features/vault/store";
import { vaultApi } from "@/features/vault/api";
import { getStorage } from "@/lib/storage";

type LoadedUrl = {
  id: string;
  url: string;
};

export function useAttachmentUrl(attachmentId: string | null): string | null {
  const sessionId = useVaultStore((state) => state.sessionId);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const attachments = useVaultStore((state) => state.data?.attachments);
  const meta = attachmentId
    ? attachments?.find((item) => item.id === attachmentId)
    : undefined;
  const [loaded, setLoaded] = useState<LoadedUrl | null>(null);

  useEffect(() => {
    if (!attachmentId || !sessionId || !vaultPath || !meta) return;

    let active = true;
    let objectUrl: string | null = null;

    void (async () => {
      try {
        const blob = await getStorage().readBlob(vaultPath, attachmentId);
        const bytes = await vaultApi.openAttachment(sessionId, attachmentId, blob);
        if (!active) return;
        objectUrl = URL.createObjectURL(
          new Blob([bytes as unknown as BlobPart], { type: meta.mime }),
        );
        setLoaded({ id: attachmentId, url: objectUrl });
      } catch {
        // A missing or unreadable image falls back to the preset.
      }
    })();

    return () => {
      active = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [attachmentId, sessionId, vaultPath, meta]);

  return attachmentId && loaded?.id === attachmentId ? loaded.url : null;
}
