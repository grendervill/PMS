import jsQR from "jsqr";

export type QrDecodeRequest = {
  id: number;
  data: Uint8ClampedArray;
  width: number;
  height: number;
};

export type QrDecodeResponse = {
  id: number;
  text: string | null;
};

const scope = self as unknown as {
  onmessage: ((event: MessageEvent<QrDecodeRequest>) => void) | null;
  postMessage: (message: QrDecodeResponse) => void;
};

scope.onmessage = (event) => {
  const { id, data, width, height } = event.data;
  let text: string | null = null;
  try {
    text = jsQR(data, width, height)?.data ?? null;
  } catch {
    text = null;
  }
  scope.postMessage({ id, text });
};
