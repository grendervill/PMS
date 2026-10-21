import jsQR from "jsqr";
import QRCode from "qrcode";

export const MAX_QR_CHARS = 2300;

export async function makeQrDataUrl(text: string, size = 320): Promise<string> {
  if (text.length > MAX_QR_CHARS) {
    throw { code: "qr_too_large", message: "Данные не помещаются в QR-код" };
  }
  try {
    return await QRCode.toDataURL(text, {
      errorCorrectionLevel: "M",
      margin: 2,
      width: size,
    });
  } catch {
    throw { code: "qr_too_large", message: "Данные не помещаются в QR-код" };
  }
}

export function decodeQrImageData(
  data: Uint8ClampedArray,
  width: number,
  height: number,
): string | null {
  const result = jsQR(data, width, height);
  return result?.data ?? null;
}

export async function decodeQrBytes(bytes: Uint8Array): Promise<string> {
  if (typeof createImageBitmap !== "function") {
    throw { code: "qr_invalid", message: "Не удалось открыть изображение" };
  }
  const bitmap = await createImageBitmap(new Blob([bytes as BlobPart]));
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) throw { code: "qr_invalid", message: "Canvas недоступен" };
    context.drawImage(bitmap, 0, 0);
    const image = context.getImageData(0, 0, canvas.width, canvas.height);
    const text = decodeQrImageData(image.data, image.width, image.height);
    if (!text) throw { code: "qr_invalid", message: "QR-код не найден" };
    return text;
  } finally {
    bitmap.close();
  }
}
