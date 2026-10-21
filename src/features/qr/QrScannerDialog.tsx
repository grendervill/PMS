import { Camera, Image as ImageIcon, Loader2, QrCode, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import { getStorage } from "@/lib/storage";
import { errorText } from "@/features/vault/errors";
import { decodeQrBytes, decodeQrImageData } from "@/features/qr/qr";
import { cn } from "@/lib/cn";

type Tab = "image" | "camera";

export function QrScannerDialog({
  open,
  onOpenChange,
  onResult,
  title = "Сканировать QR",
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onResult: (text: string) => void;
  title?: string;
}) {
  const [tab, setTab] = useState<Tab>("image");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cameraReady, setCameraReady] = useState(false);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const frameRef = useRef<number | null>(null);

  const cameraSupported =
    typeof navigator !== "undefined" &&
    typeof navigator.mediaDevices?.getUserMedia === "function";

  const finish = (text: string) => {
    onResult(text);
    onOpenChange(false);
  };

  const chooseImage = async () => {
    setBusy(true);
    setError(null);
    try {
      const path = await getStorage().pickFile("Выберите изображение с QR");
      if (!path) return;
      const bytes = await getStorage().readFile(path);
      finish(await decodeQrBytes(bytes));
    } catch (scanError) {
      setError(errorText(scanError));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!open || tab !== "camera") return;
    let cancelled = false;

    const stop = () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      streamRef.current?.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    };

    const tick = () => {
      if (cancelled) return;
      const video = videoRef.current;
      const canvas = canvasRef.current;
      if (video && canvas && video.readyState >= 2 && video.videoWidth > 0) {
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        const context = canvas.getContext("2d");
        if (context) {
          context.drawImage(video, 0, 0, canvas.width, canvas.height);
          const image = context.getImageData(0, 0, canvas.width, canvas.height);
          const text = decodeQrImageData(image.data, image.width, image.height);
          if (text) {
            stop();
            finish(text);
            return;
          }
        }
      }
      frameRef.current = requestAnimationFrame(tick);
    };

    const start = async () => {
      try {
        if (!cameraSupported) {
          setError("Камера недоступна. Выберите изображение с QR.");
          return;
        }
        const stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "environment" },
        });
        if (cancelled) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        streamRef.current = stream;
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          await videoRef.current.play();
          setCameraReady(true);
          frameRef.current = requestAnimationFrame(tick);
        }
      } catch {
        if (!cancelled) {
          setError("Не удалось включить камеру. Разрешите доступ или выберите изображение.");
        }
      }
    };

    void start();
    return () => {
      cancelled = true;
      stop();
    };
    // finish/tick are intentionally tied to this open camera session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, tab, cameraSupported]);

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description="Выберите изображение или наведите камеру на QR-код."
      className="w-[min(92vw,560px)]"
      footer={
        <Button variant="ghost" onClick={() => onOpenChange(false)}>
          <X size={15} />
          Закрыть
        </Button>
      }
    >
      <div className="grid gap-4">
        <div className="grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
          <button
            type="button"
            onClick={() => {
              setTab("image");
              setError(null);
            }}
            className={cn(
              "flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors",
              tab === "image"
                ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                : "text-[var(--muted)] hover:text-[var(--text-soft)]",
            )}
          >
            <ImageIcon size={13} />
            Изображение
          </button>
          <button
            type="button"
            onClick={() => {
              setTab("camera");
              setCameraReady(false);
              setError(null);
            }}
            disabled={!cameraSupported}
            className={cn(
              "flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors disabled:cursor-not-allowed disabled:opacity-40",
              tab === "camera"
                ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                : "text-[var(--muted)] hover:text-[var(--text-soft)]",
            )}
          >
            <Camera size={13} />
            Камера
          </button>
        </div>

        {tab === "image" ? (
          <div className="grid gap-3">
            <div className="flex min-h-44 flex-col items-center justify-center rounded-[var(--radius-md)] border border-dashed border-[var(--border)] bg-[var(--bg)]/50 px-4 py-8 text-center">
              <QrCode size={32} className="text-[var(--accent-soft)]" />
              <p className="mt-3 text-[12px] text-[var(--text-soft)]">
                PNG, JPEG или скриншот с QR-кодом
              </p>
              <Button className="mt-4" onClick={() => void chooseImage()} disabled={busy}>
                {busy ? <Loader2 size={15} className="animate-spin" /> : <ImageIcon size={15} />}
                Выбрать изображение
              </Button>
            </div>
          </div>
        ) : (
          <div className="relative overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-black">
            <video
              ref={videoRef}
              muted
              playsInline
              className="aspect-video w-full object-cover"
            />
            <canvas ref={canvasRef} className="hidden" />
            {!cameraReady ? (
              <div className="absolute inset-0 grid place-items-center text-[11px] text-[var(--muted)]">
                <span className="flex items-center gap-2">
                  <Loader2 size={14} className="animate-spin" />
                  Включаем камеру…
                </span>
              </div>
            ) : null}
            <div className="pointer-events-none absolute inset-0 grid place-items-center">
              <div className="size-40 rounded-[var(--radius-md)] border-2 border-[var(--accent)]/70" />
            </div>
          </div>
        )}

        {error ? (
          <p role="alert" className="text-[11.5px] leading-relaxed text-[var(--danger)]">
            {error}
          </p>
        ) : null}

        <p className="text-[10px] leading-relaxed text-[var(--muted)]/75">
          Сканирование происходит локально. Изображение и камера не отправляются в сеть.
        </p>
      </div>
    </Modal>
  );
}
