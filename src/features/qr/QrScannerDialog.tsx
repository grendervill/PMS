import { Camera, Image as ImageIcon, Loader2, QrCode, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import { getStorage } from "@/lib/storage";
import { errorText } from "@/features/vault/errors";
import { decodeQrBytes } from "@/features/qr/qr";
import { createFrameScanner, createQrDecoder, fitFrameSize } from "@/features/qr/qrScanner";
import { cn } from "@/lib/cn";

type Tab = "image" | "camera";

/** Reused between frames so the canvas is not allocated 60 times a second. */
function frameContext(canvas: HTMLCanvasElement, width: number, height: number) {
  canvas.width = width;
  canvas.height = height;
  return canvas.getContext("2d", { willReadFrequently: true });
}

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
  const frameCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

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
    const decoder = createQrDecoder();
    let scanner: { start(): void; stop(): void } | null = null;

    const stop = () => {
      scanner?.stop();
      scanner = null;
      decoder.dispose();
      streamRef.current?.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    };

    const capture = (): ImageData | null => {
      const video = videoRef.current;
      if (!video || video.readyState < 2 || video.videoWidth === 0) return null;

      const size = fitFrameSize(video.videoWidth, video.videoHeight);
      if (size.width === 0 || size.height === 0) return null;

      const canvas =
        frameCanvasRef.current ??
        (frameCanvasRef.current = document.createElement("canvas"));
      const context = frameContext(canvas, size.width, size.height);
      if (!context) return null;
      context.drawImage(video, 0, 0, size.width, size.height);
      return context.getImageData(0, 0, size.width, size.height);
    };

    scanner = createFrameScanner({
      capture,
      decode: (image) => decoder.decode(image),
      onResult: (text) => {
        stop();
        finish(text);
      },
    });

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
          scanner?.start();
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
    // finish is intentionally tied to this open camera session.
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
          <div className="relative aspect-[4/5] max-h-[58vh] w-full overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-black sm:aspect-video sm:max-h-none">
            {/* Видео заполняет весь контейнер: раньше оно было 16:9 внутри
                рамки 4:5, и нижняя половина оставалась чёрной — наводить
                камеру в пустоту было бессмысленно. */}
            <video
              ref={videoRef}
              muted
              playsInline
              autoPlay
              className="absolute inset-0 size-full object-cover"
            />
            {!cameraReady ? (
              <div className="absolute inset-0 grid place-items-center text-[11px] text-[var(--muted)]">
                <span className="flex items-center gap-2">
                  <Loader2 size={14} className="animate-spin" />
                  Включаем камеру…
                </span>
              </div>
            ) : null}
            <div className="pointer-events-none absolute inset-0 grid place-items-center p-3">
              <div className="aspect-square w-[min(68vw,280px)] rounded-[var(--radius-md)] border-2 border-[var(--accent)]/70 shadow-[0_0_0_999px_rgba(0,0,0,0.12)] sm:w-64" />
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
