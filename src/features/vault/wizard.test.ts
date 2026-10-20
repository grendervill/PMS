import { describe, expect, it } from "vitest";
import {
  buildSecrets,
  defaultWizardDraft,
  hasErrors,
  validateWizard,
  type WizardDraft,
} from "./wizard";

function draft(overrides: Partial<WizardDraft> = {}): WizardDraft {
  return {
    ...defaultWizardDraft,
    folder: "/vaults/personal",
    name: "Личное",
    ...overrides,
  };
}

describe("vault wizard validation", () => {
  it("requires a folder and a name", () => {
    const errors = validateWizard(draft({ folder: "", name: "" }));

    expect(errors.folder).toBeDefined();
    expect(errors.name).toBeDefined();
  });

  it("requires at least one unlock method", () => {
    const errors = validateWizard(draft());

    expect(errors.methods).toBe("Добавьте пароль-фразу или ключ-файл");
  });

  it("rejects short passphrases and mismatches", () => {
    const short = validateWizard(draft({ passphrase: "short", confirm: "short" }));
    expect(short.passphrase).toBeDefined();

    const mismatch = validateWizard(
      draft({ passphrase: "correct horse", confirm: "correct horse!" }),
    );
    expect(mismatch.confirm).toBeDefined();
  });

  it("accepts a keyfile-only setup", () => {
    const errors = validateWizard(
      draft({
        keyfile: { name: "key", bytes: new Uint8Array([1, 2, 3]), passphrase: "" },
      }),
    );

    expect(hasErrors(errors)).toBe(false);
  });

  it("builds a passphrase secret with a keyfile as the second method", () => {
    const secrets = buildSecrets(
      draft({
        passphrase: "correct horse",
        confirm: "correct horse",
        keyfile: { name: "key", bytes: new Uint8Array([1, 2, 3]), passphrase: "ssh" },
      }),
    );

    expect(secrets.primary).toEqual({ kind: "passphrase", value: "correct horse" });
    expect(secrets.secondary).toEqual({
      kind: "keyfile",
      bytesB64: "AQID",
      passphrase: "ssh",
    });
  });

  it("builds a keyfile-only primary secret", () => {
    const secrets = buildSecrets(
      draft({
        keyfile: { name: "key", bytes: new Uint8Array([4, 5]), passphrase: "" },
      }),
    );

    expect(secrets.primary).toEqual({
      kind: "keyfile",
      bytesB64: "BAU=",
      passphrase: undefined,
    });
    expect(secrets.secondary).toBeUndefined();
  });
});
