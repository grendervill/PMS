import QRCode from "qrcode";
import { describe, expect, it } from "vitest";
import { decodeQrImageData } from "./qr";

function qrPixels(text: string) {
  const qr = QRCode.create(text, { errorCorrectionLevel: "M" });
  const modules = qr.modules as unknown as {
    size: number;
    get: (row: number, col: number) => number;
  };
  const scale = 5;
  const width = modules.size * scale;
  const data = new Uint8ClampedArray(width * width * 4);
  for (let row = 0; row < modules.size; row += 1) {
    for (let col = 0; col < modules.size; col += 1) {
      const dark = modules.get(row, col) === 1;
      for (let dy = 0; dy < scale; dy += 1) {
        for (let dx = 0; dx < scale; dx += 1) {
          const index = ((row * scale + dy) * width + col * scale + dx) * 4;
          const value = dark ? 0 : 255;
          data[index] = value;
          data[index + 1] = value;
          data[index + 2] = value;
          data[index + 3] = 255;
        }
      }
    }
  }
  return { data, width };
}

describe("QR decoding", () => {
  it("decodes a generated QR image", () => {
    const text = "otpauth://totp/GitHub:alex?secret=JBSWY3DPEHPK3PXP";
    const { data, width } = qrPixels(text);
    expect(decodeQrImageData(data, width, width)).toBe(text);
  });

  it("returns null for an image without a QR code", () => {
    const data = new Uint8ClampedArray(40 * 40 * 4);
    data.fill(255);
    expect(decodeQrImageData(data, 40, 40)).toBeNull();
  });
});
