import { Download, Loader2, QrCode } from "lucide-react";
import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import { useToast } from "@/components/ui/toast-context";
import { makeQrDataUrl } from "@/features/qr/qr";
import { totpUri } from "@/features/totp/otpauth";
import { errorText } from "@/features/vault/errors";
import type { TotpConfig } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";

function dataUrlBytes(value: string): Uint8Array {
  const binary = atob(value.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function TotpQrDialog({
  config,
  onClose,
}: {
  config: TotpConfig;
  onClose: () => void;
}) {
  const toast = useToast();
  const [image, setImage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let active = true;
    void makeQrDataUrl(totpUri(config), 280)
      .then((value) => {
        if (active) setImage(value);
      })
      .catch((generateError) => {
        if (active) setError(errorText(generateError));
      });
    return () => {
      active = false;
    };
  }, [config]);

  const download = async () => {
    if (!image) return;
    setSaving(true);
    try {
      const path = await save({ defaultPath: "totp-qr.png" });
      if (!path) return;
      await getStorage().saveFile(path, dataUrlBytes(image));
      toast.show({ title: "QR-код сохранён", description: path, tone: "success" });
    } catch (saveError) {
      toast.show({
        title: "Не удалось сохранить QR",
        description: errorText(saveError),
        tone: "error",
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="TOTP QR"
      description="Отсканируйте код в другом приложении, поддерживающем TOTP."
      className="w-[min(92vw,460px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Закрыть
          </Button>
          <Button variant="secondary" onClick={() => void download()} disabled={!image || saving}>
            {saving ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />}
            Сохранить PNG
          </Button>
        </>
      }
    >
      <div className="grid place-items-center gap-3">
        {image ? (
          <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-white p-3">
            <img src={image} alt="TOTP QR" className="size-60" />
          </div>
        ) : error ? (
          <p className="text-[11.5px] text-[var(--danger)]">{error}</p>
        ) : (
          <div className="grid min-h-40 place-items-center">
            <Loader2 size={22} className="animate-spin text-[var(--accent-soft)]" />
          </div>
        )}
        <p className="flex items-center gap-1.5 text-[10px] text-[var(--muted)]">
          <QrCode size={12} />
          Секрет TOTP виден в QR. Не показывайте код посторонним.
        </p>
      </div>
    </Modal>
  );
}
