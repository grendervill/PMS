import type { Page } from "@playwright/test";

/**
 * Installs a small in-page Tauri IPC mock so the React UI can be exercised
 * in Chromium. Crypto and file-format behavior are covered by Rust tests;
 * this harness tests the UI flows and command wiring.
 */
export async function installMockTauri(page: Page) {
  await page.addInitScript(() => {
    const state: {
      vault: null | {
        data: Record<string, unknown>;
        passphrase: string | null;
        slotKinds: string[];
        createdAt: string;
      };
      session: string | null;
      files: Map<string, string>;
      attachments: Map<string, string>;
      passwordIndex: number;
      biometricAvailable: boolean;
      biometricFailures: number;
    } = {
      vault: null,
      session: null,
      files: new Map(),
      attachments: new Map(),
      passwordIndex: 0,
      biometricAvailable: true,
      biometricFailures: 0,
    };

    const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

    const makeData = (name: string, id: string, createdAt: string) => ({
      schemaVersion: 1,
      id,
      name,
      createdAt,
      updatedAt: createdAt,
      settings: {
        autoLockMinutes: 5,
        clipboardClearSeconds: 30,
        backupCount: 20,
        faviconFetch: true,
        maxAttachmentMb: 50,
      },
      entries: [],
      sshKeys: [],
      attachments: [],
      drafts: [],
    });

    const requireVault = () => {
      if (!state.vault) {
        throw { code: "bad_vault", message: "Хранилище не создано" };
      }
      return state.vault;
    };

    const invoke = async (command: string, args: Record<string, unknown> = {}) => {
      const paths = args.paths as string[];
      const options = args.options as { directory?: boolean } | undefined;
      const text = args.text as string | undefined;
      const path = args.path as string;
      const b64 = args.b64 as string;
      const config = args.config as { name: string };
      const secret = args.secret as { kind: string; value?: string };
      const data = args.data as Record<string, unknown>;
      const w = window as unknown as { __E2E_CLIPBOARD__?: string };

      switch (command) {
        case "plugin:path|join":
          return paths.join("/");
        case "plugin:path|home_dir":
          return "/tmp/pms-home";
        case "plugin:path|resolve_directory":
          return "/tmp/pms-home";
        case "plugin:dialog|open":
          if (options?.directory) return "/tmp/pms-e2e-vault";
          state.files.set(
            "/tmp/pms-e2e-attachment.txt",
            "aGVsbG8gYXR0YWNobWVudA==",
          );
          return "/tmp/pms-e2e-attachment.txt";
        case "plugin:dialog|save":
          return "/tmp/pms-e2e-download.txt";
        case "plugin:clipboard-manager|write_text":
          w.__E2E_CLIPBOARD__ = text;
          return null;
        case "plugin:clipboard-manager|read_text":
          return w.__E2E_CLIPBOARD__ ?? "";
        case "plugin:opener|open_url":
          return null;
        case "storage_exists":
          return state.files.has(path);
        case "storage_mkdir":
          return null;
        case "storage_write_atomic":
          state.files.set(path, b64);
          return null;
        case "storage_read_file": {
          const value = state.files.get(path);
          if (value === undefined) {
            throw { code: "storage", message: "Файл не найден" };
          }
          return { b64: value };
        }
        case "storage_stat":
          return { isFile: true, isDir: false, size: 1, mtimeMs: Date.now() };
        case "storage_list_dir":
          return [];
        case "storage_remove_file":
          state.files.delete(path);
          return null;
        case "vault_create": {
          const now = new Date().toISOString();
          const id = "vault-e2e";
          state.vault = {
            data: makeData(config.name, id, now),
            passphrase: secret.kind === "passphrase" ? (secret.value ?? "") : null,
            slotKinds: [secret.kind],
            createdAt: now,
          };
          state.session = "session-e2e";
          state.files.set("/tmp/pms-e2e-vault/pms.vault", "ZmFrZQ==");
          return {
            sessionId: state.session,
            fileB64: "ZmFrZQ==",
            data: clone(state.vault.data),
          };
        }
        case "vault_inspect": {
          const vault = requireVault();
          return {
            vaultId: vault.data.id,
            format: 1,
            createdAt: vault.createdAt,
            cipher: "xchacha20-poly1305",
            slotKinds: vault.slotKinds,
            fileHash: "hash",
          };
        }
        case "vault_unlock": {
          const vault = requireVault();
          if (secret.kind !== "passphrase" || secret.value !== vault.passphrase) {
            throw { code: "invalid_credentials", message: "Неверный пароль" };
          }
          state.session = "session-e2e";
          return { sessionId: state.session, data: clone(vault.data) };
        }
        case "vault_save": {
          const vault = requireVault();
          if (!state.session) {
            throw { code: "session_not_found", message: "Сессия не найдена" };
          }
          vault.data = clone(data);
          return { fileB64: "ZmFrZQ==" };
        }
        case "vault_lock":
          state.session = null;
          return null;
        case "generate_password": {
          const values = ["A7!kP9#vQ2$mR5@x", "B8!nT3%wZ6^cL4&y"];
          const value = values[state.passwordIndex % values.length];
          state.passwordIndex += 1;
          return value;
        }
        case "generate_login":
          return "alex_generated_42";
        case "vault_rewrap":
          return { fileB64: "ZmFrZQ==" };
        case "vault_remove_slot":
          return { fileB64: "ZmFrZQ==" };
        case "attachment_seal": {
          const attachmentId = "attachment-1";
          state.attachments.set(attachmentId, b64);
          return {
            blobB64: b64,
            meta: {
              id: attachmentId,
              name: args.name,
              mime: args.mime,
              size: Math.floor((b64.length * 3) / 4),
              sha256: "hash",
              kind: String(args.mime).startsWith("image/")
                ? "image"
                : String(args.mime) === "application/pdf"
                  ? "pdf"
                  : String(args.mime).startsWith("text/")
                    ? "text"
                    : "other",
              createdAt: new Date().toISOString(),
            },
          };
        }
        case "attachment_open":
          return { b64: state.attachments.get(String(args.attachmentId)) ?? "" };
        case "totp_inspect":
          return {
            secret: String(args.input).startsWith("otpauth")
              ? new URL(String(args.input)).searchParams.get("secret")
              : String(args.input),
            algorithm: "SHA1",
            digits: 6,
            period: 30,
            issuer: "E2E",
            account: "alex",
          };
        case "totp_code":
          return { code: "123456", remaining: 30 };
        case "ssh_generate": {
          const sshOptions = args.options as {
            keyType?: string;
            rsaBits?: number | null;
            curve?: string | null;
          };
          return {
            keyType: sshOptions.keyType ?? "ed25519",
            rsaBits: sshOptions.rsaBits ?? null,
            curve: sshOptions.curve ?? null,
            publicKey:
              "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMockPmsSshKeyForE2eTestsOnly alex@e2e",
            privateKey:
              "-----BEGIN OPENSSH PRIVATE KEY-----\nbW9jay1zc2gta2V5\n-----END OPENSSH PRIVATE KEY-----\n",
            fingerprint: "SHA256:mockPmsSshFingerprintForE2eTests",
          };
        }
        case "ssh_formats":
          return {
            publicKey:
              "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMockPmsSshKeyForE2eTestsOnly alex@e2e",
            openssh:
              "-----BEGIN OPENSSH PRIVATE KEY-----\nbW9jay1zc2gta2V5\n-----END OPENSSH PRIVATE KEY-----\n",
            pem: "-----BEGIN PRIVATE KEY-----\nbW9jay1wZW0=\n-----END PRIVATE KEY-----\n",
            pkcs8: "-----BEGIN PRIVATE KEY-----\nbW9jay1wa2NzOA==\n-----END PRIVATE KEY-----\n",
            fingerprint: "SHA256:mockPmsSshFingerprintForE2eTests",
          };
        case "ssh_install": {
          const directory = String(args.directory);
          const name = String(args.name);
          return {
            privatePath: `${directory}/${name}`,
            publicPath: `${directory}/${name}.pub`,
            privateMode: 384,
            publicMode: 420,
            directoryMode: 448,
            ownedByCurrentUser: true,
            warning: null,
          };
        }
        case "ssh_check":
          return {
            exists: true,
            privateMode: 384,
            publicMode: 420,
            directoryMode: 448,
            ownedByCurrentUser: true,
            ok: true,
          };
        case "biometric_status":
          return { available: state.biometricAvailable };
        case "biometric_enroll":
          return null;
        case "biometric_disable":
          return null;
        case "biometric_unlock": {
          const w = window as unknown as {
            __E2E_BIOMETRIC_FAILURES__?: number;
            __E2E_BIOMETRIC_CANCEL__?: boolean;
          };
          if (w.__E2E_BIOMETRIC_CANCEL__) {
            throw { code: "biometric_cancelled", message: "Отменено" };
          }
          if ((w.__E2E_BIOMETRIC_FAILURES__ ?? 0) > 0) {
            w.__E2E_BIOMETRIC_FAILURES__ = (w.__E2E_BIOMETRIC_FAILURES__ ?? 0) - 1;
            throw { code: "biometric_failed", message: "Не распознано" };
          }
          const vault = requireVault();
          state.session = "session-e2e";
          return { sessionId: state.session, data: clone(vault.data) };
        }
        default:
          throw { code: "unknown_command", message: command };
      }
    };

    Object.assign(window, {
      __TAURI_INTERNALS__: { invoke },
    });
  });
}
