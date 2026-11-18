import { decodeQrImageData } from "@/features/qr/qr";
import QrWorkerFactory from "@/features/qr/qrDecoder.worker?worker&inline";

/** At most ~7 analyses per second: most camera frames are simply skipped. */
export const QR_SCAN_INTERVAL_MS = 140;
/** Longest side of the downscaled frame that gets analysed. */
export const QR_MAX_FRAME_SIDE = 640;
/** A worker that never answers (blocked by CSP, crashed) must not stall the loop. */
const WORKER_TIMEOUT_MS = 4000;

export type QrDecoderKind = "barcode-detector" | "worker" | "sync";

export type QrDecoder = {
  readonly kind: QrDecoderKind;
  decode(image: ImageData): Promise<string | null>;
  dispose(): void;
};

type DetectedBarcode = { rawValue: string };

type BarcodeDetectorLike = {
  detect(source: ImageData): Promise<DetectedBarcode[]>;
};

type BarcodeDetectorConstructor = new (options?: { formats?: string[] }) => BarcodeDetectorLike;

function barcodeDetectorConstructor(): BarcodeDetectorConstructor | null {
  if (typeof window === "undefined") return null;
  const candidate = (window as unknown as { BarcodeDetector?: unknown }).BarcodeDetector;
  return typeof candidate === "function" ? (candidate as BarcodeDetectorConstructor) : null;
}

export function hasBarcodeDetector(): boolean {
  return barcodeDetectorConstructor() !== null;
}

/** Keeps the analysed frame small: jsQR cost grows with the pixel count. */
export function fitFrameSize(
  width: number,
  height: number,
  maxSide = QR_MAX_FRAME_SIDE,
): { width: number; height: number } {
  if (width <= 0 || height <= 0) return { width: 0, height: 0 };
  const longest = Math.max(width, height);
  if (longest <= maxSide) return { width, height };
  const scale = maxSide / longest;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

function decodeInline(image: ImageData): string | null {
  return decodeQrImageData(image.data, image.width, image.height);
}

function createBarcodeDetectorDecoder(Detector: BarcodeDetectorConstructor): QrDecoder {
  const detector = new Detector({ formats: ["qr_code"] });
  return {
    kind: "barcode-detector",
    async decode(image) {
      try {
        const codes = await detector.detect(image);
        return codes[0]?.rawValue ?? null;
      } catch {
        return null;
      }
    },
    dispose() {
      // The platform owns the detector.
    },
  };
}

function createSyncDecoder(): QrDecoder {
  return {
    kind: "sync",
    decode: async (image) => decodeInline(image),
    dispose() {
      // Nothing to release.
    },
  };
}

function createWorkerDecoder(): QrDecoder | null {
  if (typeof Worker !== "function") return null;

  let worker: Worker;
  try {
    worker = new QrWorkerFactory();
  } catch {
    return null;
  }

  const handlers = new Map<number, (text: string | null) => void>();
  let nextId = 0;
  let broken = false;

  const terminate = () => {
    try {
      worker.terminate();
    } catch {
      // Already gone.
    }
  };

  const settleAll = (text: string | null) => {
    for (const handler of handlers.values()) handler(text);
    handlers.clear();
  };

  worker.onmessage = (event: MessageEvent<{ id: number; text: string | null }>) => {
    const handler = handlers.get(event.data.id);
    if (!handler) return;
    handlers.delete(event.data.id);
    handler(event.data.text);
  };

  // A worker blocked by CSP fires `error` instead of answering. Fall back to
  // the synchronous decoder so scanning keeps working.
  worker.onerror = () => {
    broken = true;
    settleAll(null);
    terminate();
  };

  return {
    kind: "worker",
    decode(image) {
      if (broken) return Promise.resolve(decodeInline(image));

      return new Promise<string | null>((resolve) => {
        const id = nextId;
        nextId += 1;

        const timer = setTimeout(() => {
          handlers.delete(id);
          broken = true;
          terminate();
          resolve(decodeInline(image));
        }, WORKER_TIMEOUT_MS);

        handlers.set(id, (text) => {
          clearTimeout(timer);
          resolve(text);
        });

        try {
          worker.postMessage({
            id,
            data: image.data,
            width: image.width,
            height: image.height,
          });
        } catch {
          clearTimeout(timer);
          handlers.delete(id);
          broken = true;
          resolve(decodeInline(image));
        }
      });
    },
    dispose() {
      broken = true;
      settleAll(null);
      terminate();
    },
  };
}

/**
 * Prefers the platform detector, then the jsQR worker, then a synchronous
 * jsQR call (no `Worker` support, e.g. an old WebView).
 */
export function createQrDecoder(): QrDecoder {
  const Detector = barcodeDetectorConstructor();
  if (Detector) {
    try {
      return createBarcodeDetectorDecoder(Detector);
    } catch {
      // Unsupported format list: keep going with jsQR.
    }
  }
  return createWorkerDecoder() ?? createSyncDecoder();
}

export type FrameScannerOptions = {
  capture: () => ImageData | null;
  decode: (image: ImageData) => Promise<string | null>;
  onResult: (text: string) => void;
  onError?: (error: unknown) => void;
  intervalMs?: number;
  now?: () => number;
  schedule?: (callback: () => void) => number;
  cancel?: (handle: number) => void;
};

export type FrameScanner = {
  start(): void;
  stop(): void;
};

/**
 * Drives the camera loop on animation frames. A frame is analysed only when
 * the interval has passed and the previous analysis already finished, so the
 * decode never overlaps itself and never blocks the frame callback.
 */
export function createFrameScanner(options: FrameScannerOptions): FrameScanner {
  const interval = options.intervalMs ?? QR_SCAN_INTERVAL_MS;
  const now = options.now ?? (() => Date.now());
  const schedule = options.schedule ?? ((callback: () => void) => requestAnimationFrame(callback));
  const cancel = options.cancel ?? ((handle: number) => cancelAnimationFrame(handle));

  let handle: number | null = null;
  let stopped = false;
  let busy = false;
  let lastScanAt = Number.NEGATIVE_INFINITY;

  const stop = () => {
    stopped = true;
    if (handle !== null) {
      cancel(handle);
      handle = null;
    }
  };

  const tick = () => {
    handle = null;
    if (stopped) return;

    if (!busy && now() - lastScanAt >= interval) {
      lastScanAt = now();
      let image: ImageData | null = null;
      try {
        image = options.capture();
      } catch (error) {
        options.onError?.(error);
      }
      if (image) {
        busy = true;
        void options
          .decode(image)
          .then((text) => {
            if (!stopped && text) {
              stop();
              options.onResult(text);
            }
          })
          .catch((error: unknown) => {
            if (!stopped) options.onError?.(error);
          })
          .finally(() => {
            busy = false;
          });
      }
    }

    if (!stopped) handle = schedule(tick);
  };

  return {
    start() {
      if (stopped || handle !== null) return;
      handle = schedule(tick);
    },
    stop,
  };
}
