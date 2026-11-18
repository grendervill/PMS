import { invoke } from "@tauri-apps/api/core";
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

/**
 * Sends raw bytes as the whole IPC body.
 *
 * Tauri 2 accepts `invoke(cmd, bytes, { headers })` on desktop and hands the
 * bytes to Rust as `InvokeBody::Raw`, so a vault file no longer has to be
 * base64-encoded (+33% of bytes and a few tens of ms per MiB) just to cross the
 * IPC boundary. Android cannot do this (`InvokeBody::Raw` never arrives there),
 * so the caller decides which command to use.
 *
 * Extra parameters have no place in a raw body: headers carry them.
 */
export function invokeRawBytes<T>(
  command: string,
  bytes: Uint8Array,
  headers: Record<string, string>,
): Promise<T> {
  return invoke<T>(command, bytes, { headers });
}
