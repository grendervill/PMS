import { toBase64 } from "@/lib/base64";
import type {
  CipherSuite,
  KdfProfile,
  UnlockSecret,
} from "@/features/vault/types";

export type KeyfileSelection = {
  name: string;
  bytes: Uint8Array;
  passphrase: string;
};

export type WizardDraft = {
  folder: string;
  name: string;
  passphrase: string;
  confirm: string;
  keyfile: KeyfileSelection | null;
  kdfProfile: KdfProfile;
  cipher: CipherSuite;
};

export type WizardErrorKey =
  | "folder"
  | "name"
  | "passphrase"
  | "confirm"
  | "methods"
  | "keyfile";

export type WizardErrors = Partial<Record<WizardErrorKey, string>>;

export function validateWizard(draft: WizardDraft): WizardErrors {
  const errors: WizardErrors = {};

  if (!draft.folder.trim()) {
    errors.folder = "Выберите папку для хранилища";
  }
  if (!draft.name.trim()) {
    errors.name = "Введите название хранилища";
  }

  if (draft.passphrase) {
    if (draft.passphrase.length < 8) {
      errors.passphrase = "Пароль-фраза должна быть не короче 8 символов";
    }
    if (draft.passphrase !== draft.confirm) {
      errors.confirm = "Пароль-фразы не совпадают";
    }
  }

  if (!draft.passphrase && !draft.keyfile) {
    errors.methods = "Добавьте пароль-фразу или ключ-файл";
  }

  if (draft.keyfile && draft.keyfile.bytes.length === 0) {
    errors.keyfile = "Ключ-файл пуст";
  }

  return errors;
}

export function hasErrors(errors: WizardErrors): boolean {
  return Object.keys(errors).length > 0;
}

export function buildSecrets(draft: WizardDraft): {
  primary: UnlockSecret;
  secondary?: UnlockSecret;
} {
  const keyfileSecret: UnlockSecret | null = draft.keyfile
    ? {
        kind: "keyfile",
        bytesB64: toBase64(draft.keyfile.bytes),
        passphrase: draft.keyfile.passphrase || undefined,
      }
    : null;

  if (draft.passphrase) {
    return {
      primary: { kind: "passphrase", value: draft.passphrase },
      secondary: keyfileSecret ?? undefined,
    };
  }

  if (keyfileSecret) {
    return { primary: keyfileSecret };
  }

  throw new Error("validateWizard must be called before buildSecrets");
}

export const defaultWizardDraft: WizardDraft = {
  folder: "",
  name: "",
  passphrase: "",
  confirm: "",
  keyfile: null,
  kdfProfile: "standard",
  cipher: "xchacha20-poly1305",
};
