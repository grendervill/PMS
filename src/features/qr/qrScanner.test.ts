import { afterEach, describe, expect, it, vi } from "vitest";
import {
  QR_MAX_FRAME_SIDE,
  QR_SCAN_INTERVAL_MS,
  createFrameScanner,
  createQrDecoder,
  fitFrameSize,
  hasBarcodeDetector,
} from "./qrScanner";

function fakeImage(width = 4, height = 4): ImageData {
  return {
    data: new Uint8ClampedArray(width * height * 4),
    width,
    height,
  } as ImageData;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("fitFrameSize", () => {
  it("scales the long side down to the maximum", () => {
    expect(fitFrameSize(1920, 1080)).toEqual({ width: 640, height: 360 });
    expect(fitFrameSize(1080, 1920)).toEqual({ width: 360, height: 640 });
  });

  it("keeps small frames untouched", () => {
    expect(fitFrameSize(320, 240)).toEqual({ width: 320, height: 240 });
    expect(fitFrameSize(0, 240)).toEqual({ width: 0, height: 0 });
    expect(Math.max(...Object.values(fitFrameSize(4000, 3000)))).toBeLessThanOrEqual(
      QR_MAX_FRAME_SIDE,
    );
  });
});

describe("createQrDecoder", () => {
  it("uses BarcodeDetector when the platform provides one", async () => {
    const detect = vi.fn(async () => [{ rawValue: "pms://shared" }]);
    const constructor = vi.fn();

    class FakeBarcodeDetector {
      detect = detect;
      constructor(options?: unknown) {
        constructor(options);
      }
    }

    vi.stubGlobal("BarcodeDetector", FakeBarcodeDetector);

    expect(hasBarcodeDetector()).toBe(true);
    const decoder = createQrDecoder();
    expect(decoder.kind).toBe("barcode-detector");
    expect(constructor).toHaveBeenCalledWith({ formats: ["qr_code"] });

    const image = fakeImage();
    await expect(decoder.decode(image)).resolves.toBe("pms://shared");
    expect(detect).toHaveBeenCalledWith(image);
  });

  it("ignores a BarcodeDetector that throws on results", async () => {
    vi.stubGlobal(
      "BarcodeDetector",
      class {
        detect = vi.fn(async () => {
          throw new Error("unsupported");
        });
      },
    );

    const decoder = createQrDecoder();
    await expect(decoder.decode(fakeImage())).resolves.toBeNull();
  });

  it("falls back to a synchronous decode without Worker support", () => {
    const decoder = createQrDecoder();
    expect(decoder.kind).toBe(typeof Worker === "function" ? "worker" : "sync");
    decoder.dispose();
  });

  it("runs jsQR inside a Worker when one is available", async () => {
    const instances: FakeWorker[] = [];

    class FakeWorker {
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;
      static readonly created: FakeWorker[] = instances;

      constructor() {
        instances.push(this);
      }

      postMessage(message: { id: number }) {
        queueMicrotask(() => this.onmessage?.({ data: { id: message.id, text: "from-worker" } } as MessageEvent));
      }

      terminate() {
        // no-op
      }
    }

    vi.stubGlobal("Worker", FakeWorker);

    const decoder = createQrDecoder();
    expect(decoder.kind).toBe("worker");
    await expect(decoder.decode(fakeImage())).resolves.toBe("from-worker");
    expect(instances).toHaveLength(1);
    decoder.dispose();
  });
});

describe("createFrameScanner", () => {
  function manualClock() {
    let clock = 0;
    let pending: (() => void) | null = null;
    const cancelled: number[] = [];
    return {
      now: () => clock,
      schedule: (callback: () => void) => {
        pending = callback;
        return 1;
      },
      cancel: (handle: number) => {
        cancelled.push(handle);
        pending = null;
      },
      /** Advances time like a 60 fps camera and runs the pending frame. */
      frame(ms = 16) {
        clock += ms;
        const callback = pending;
        pending = null;
        callback?.();
      },
      get hasPending() {
        return pending !== null;
      },
      cancelled,
    };
  }

  it("analyses at most one frame per scan interval", async () => {
    const clock = manualClock();
    const scans: number[] = [];

    const scanner = createFrameScanner({
      capture: () => fakeImage(),
      decode: async () => {
        scans.push(clock.now());
        return null;
      },
      onResult: () => undefined,
      now: clock.now,
      schedule: clock.schedule,
      cancel: clock.cancel,
    });

    scanner.start();
    for (let frame = 0; frame < 120; frame += 1) {
      clock.frame(16);
      for (let step = 0; step < 5; step += 1) await Promise.resolve();
    }

    // 120 frames * 16 ms = 1.92 s, so at most ~14 analyses fit the interval.
    expect(scans.length).toBeGreaterThan(1);
    expect(scans.length).toBeLessThanOrEqual(Math.ceil((120 * 16) / QR_SCAN_INTERVAL_MS) + 1);
    for (let index = 1; index < scans.length; index += 1) {
      expect(scans[index]! - scans[index - 1]!).toBeGreaterThanOrEqual(QR_SCAN_INTERVAL_MS);
    }
  });

  it("does not start a second analysis while one is still running", async () => {
    const clock = manualClock();
    let running = 0;
    let decodeCalls = 0;
    const resolvers: (() => void)[] = [];

    const scanner = createFrameScanner({
      capture: () => fakeImage(),
      decode: () => {
        decodeCalls += 1;
        running += 1;
        return new Promise<string | null>((resolve) => {
          resolvers.push(() => {
            running -= 1;
            resolve(null);
          });
        });
      },
      onResult: () => undefined,
      now: clock.now,
      schedule: clock.schedule,
      cancel: clock.cancel,
    });

    scanner.start();
    for (let frame = 0; frame < 100; frame += 1) clock.frame(16);

    expect(decodeCalls).toBe(1);
    expect(running).toBe(1);

    resolvers.forEach((resolve) => resolve());
    for (let step = 0; step < 5; step += 1) await Promise.resolve();
    for (let frame = 0; frame < 20; frame += 1) clock.frame(16);
    expect(decodeCalls).toBe(2);
  });

  it("stops the loop when a code is found", async () => {
    const clock = manualClock();
    const results: string[] = [];
    const scanner = createFrameScanner({
      capture: () => fakeImage(),
      decode: async () => "pms://shared",
      onResult: (text) => results.push(text),
      now: clock.now,
      schedule: clock.schedule,
      cancel: clock.cancel,
    });

    scanner.start();
    for (let frame = 0; frame < 20; frame += 1) {
      clock.frame(16);
      for (let step = 0; step < 5; step += 1) await Promise.resolve();
    }

    expect(results).toEqual(["pms://shared"]);
    expect(clock.hasPending).toBe(false);
    expect(clock.cancelled).toHaveLength(1);
  });

  it("stops scheduling frames after stop()", () => {
    const clock = manualClock();
    const scanner = createFrameScanner({
      capture: () => null,
      decode: async () => null,
      onResult: () => undefined,
      now: clock.now,
      schedule: clock.schedule,
      cancel: clock.cancel,
    });

    scanner.start();
    expect(clock.hasPending).toBe(true);
    scanner.stop();
    expect(clock.cancelled).toHaveLength(1);
    expect(clock.hasPending).toBe(false);
  });
});
