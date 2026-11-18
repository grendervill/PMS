import { fromBase64 } from "@/lib/base64";

function isArrayBuffer(value: unknown): value is ArrayBuffer {
  // `instanceof` fails across realms (jsdom, workers), so check the tag too.
  return (
    value instanceof ArrayBuffer ||
    Object.prototype.toString.call(value) === "[object ArrayBuffer]"
  );
}

/**
 * Normalizes an IPC payload to bytes.
 *
 * Commands that return `tauri::ipc::Response` arrive as an `ArrayBuffer`. Tests
 * and the Android fallbacks may hand over a `Uint8Array`, a plain number array
 * or a base64 string, so all of them are accepted here.
 */
export function toBytes(raw: unknown): Uint8Array {
  if (raw instanceof Uint8Array) return raw;
  if (ArrayBuffer.isView(raw)) {
    return new Uint8Array(raw.buffer, raw.byteOffset, raw.byteLength);
  }
  if (isArrayBuffer(raw)) return new Uint8Array(raw);
  if (Array.isArray(raw)) return new Uint8Array(raw as number[]);
  if (typeof raw === "string") return fromBase64(raw);
  throw new Error("Unexpected IPC payload");
}
