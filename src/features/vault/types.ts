export type CipherSuite = "xchacha20-poly1305" | "aes-256-gcm";
export type KdfProfile = "standard" | "strong" | "paranoid";
export type KdfSlotKind = "passphrase" | "keyfile" | "passphrase-keyfile";
export type EntryKind = "login";

export type HistoryItem = {
  value: string;
  changedAt: string;
};

export type TotpConfig = {
  secret: string;
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: 6 | 8;
  period: 30 | 60;
  issuer?: string | null;
  account?: string | null;
};

export type Entry = {
  id: string;
  kind: EntryKind;
  schemaVersion: number;
  name: string;
  login: string;
  password: string;
  url: string;
  urls: string[];
  comment: string;
  totp?: TotpConfig | null;
  attachmentIds: string[];
  sshKeyIds: string[];
  loginHistory: HistoryItem[];
  passwordHistory: HistoryItem[];
  createdAt: string;
  updatedAt: string;
};

export type SshKey = {
  id: string;
  name: string;
  type: "ed25519" | "rsa" | "ecdsa";
  rsaBits?: number | null;
  curve?: string | null;
  publicKey: string;
  fingerprint?: string | null;
  privateKey: string;
  passphrase?: string | null;
  comment: string;
  attachmentIds: string[];
  createdAt: string;
  updatedAt: string;
};

export type SshKeyType = SshKey["type"];

export type CardImagePreset =
  | "emerald"
  | "graphite"
  | "midnight"
  | "oxblood"
  | "sand"
  | "copper";

export type BankCard = {
  id: string;
  schemaVersion: number;
  number: string;
  expiry: string;
  cvv: string;
  holder: string;
  purpose: string;
  country: string;
  address: string;
  postalCode: string;
  imagePreset: CardImagePreset;
  imageAttachmentId: string | null;
  createdAt: string;
  updatedAt: string;
};

export type BankCardDraft = Omit<
  BankCard,
  "id" | "schemaVersion" | "createdAt" | "updatedAt"
>;

export type DocumentKind = "passport" | "snils" | "driver-license";
export type PassportKind = "main" | "foreign";

export type VaultDocument = {
  id: string;
  schemaVersion: number;
  kind: DocumentKind;
  passportKind: PassportKind | null;
  fullName: string;
  series: string;
  number: string;
  issuedBy: string;
  departmentCode: string;
  issuedAt: string;
  registrationAddress: string;
  validUntil: string;
  categories: string;
  birthDate: string;
  birthPlace: string;
  attachmentIds: string[];
  createdAt: string;
  updatedAt: string;
};

export type DocumentDraft = Omit<
  VaultDocument,
  "id" | "schemaVersion" | "createdAt" | "updatedAt"
>;

export type AttachmentMeta = {
  id: string;
  name: string;
  mime: string;
  size: number;
  sha256: string;
  kind: "image" | "pdf" | "text" | "other";
  createdAt: string;
};

export type Draft = {
  id: string;
  kind: "entry" | "ssh";
  payload: unknown;
  updatedAt: string;
};

export type VaultSettings = {
  autoLock?: import("./autoLock").AutoLockOption;
  cardLayout?: import("../entries/cardLayout").CardLayout;
  autoLockMinutes: number;
  clipboardClearSeconds: number;
  backupRetentionDays: number;
  autoBackupDays: number;
  faviconFetch: boolean;
  maxAttachmentMb: number;
};

export type VaultData = {
  schemaVersion: number;
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  settings: VaultSettings;
  entries: Entry[];
  sshKeys: SshKey[];
  bankCards: BankCard[];
  documents: VaultDocument[];
  attachments: AttachmentMeta[];
  drafts: Draft[];
};

export type VaultPublicInfo = {
  vaultId: string;
  format: number;
  createdAt: string;
  cipher: CipherSuite;
  slotKinds: KdfSlotKind[];
  fileHash: string;
};

export type UnlockSecret =
  | { kind: "passphrase"; value: string }
  | { kind: "keyfile"; bytesB64: string; passphrase?: string }
  | {
      kind: "passphraseKeyfile";
      value: string;
      bytesB64: string;
      passphrase?: string;
    };

export type CreateVaultInput = {
  folder: string;
  name: string;
  kdfProfile: KdfProfile;
  cipher: CipherSuite;
  primarySecret: UnlockSecret;
  secondarySecret?: UnlockSecret;
};

export type VaultErrorInfo = {
  code: string;
  message: string;
};

export type VaultStatus = "welcome" | "locked" | "unlocking" | "unlocked";

export function defaultVaultSettings(): VaultSettings {
  return {
    autoLock: "5m",
    cardLayout: "list",
    autoLockMinutes: 5,
    clipboardClearSeconds: 30,
    backupRetentionDays: 7,
    autoBackupDays: 1,
    faviconFetch: true,
    maxAttachmentMb: 50,
  };
}
