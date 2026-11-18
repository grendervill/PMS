export type PasswordOptions = {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  customSymbols?: string | null;
  excludeAmbiguous: boolean;
  /** Characters the site rejects; omitted from every enabled set. */
  excludeChars?: string | null;
  /** Minimum number of digits in the result (0 = no requirement). */
  minDigits: number;
  /** Minimum number of characters from the symbols set (0 = no requirement). */
  minSymbols: number;
};

export type LoginOptions = PasswordOptions;

/** Where the optional digit goes when `includeNumber` is on. */
export type NumberPosition = "start" | "end";

/** Which word list the passphrase is built from. */
export type WordlistSource = "eff" | "custom";

export type PassphraseOptions = {
  words: number;
  separator: string;
  capitalize: boolean;
  includeNumber: boolean;
  numberPosition: NumberPosition;
  wordlist: WordlistSource;
};

/** Which value the "Пароль" mode produces. */
export type PasswordKind = "random" | "readable" | "pin";

/** Numeric PIN. Mirrors `PinOptions` in `core/generator.rs`. */
export type PinOptions = {
  length: number;
  /** Reject 1111, 1234, 9876 and other trivially guessable sequences. */
  excludeRepeats: boolean;
};

/** Syllable-based value. Mirrors `PronounceableOptions` in `core/generator.rs`. */
export type PronounceableOptions = {
  length: number;
  capitalize: boolean;
};

/** Mirrors `WordlistInfo` in `commands/generator.rs`. */
export type WordlistInfo = {
  customWords: number | null;
  builtinWords: number;
  minCustomWords: number;
};

/** Mirrors the tag union the Rust `generator_entropy` command expects. */
export type EntropyInput =
  | { mode: "password"; options: PasswordOptions }
  | { mode: "login"; options: LoginOptions }
  | { mode: "passphrase"; options: PassphraseOptions }
  | { mode: "pin"; options: PinOptions }
  | { mode: "pronounceable"; options: PronounceableOptions };

export const defaultPasswordOptions: PasswordOptions = {
  length: 16,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  customSymbols: null,
  excludeAmbiguous: true,
  excludeChars: null,
  minDigits: 0,
  minSymbols: 0,
};

export const defaultLoginOptions: PasswordOptions = {
  length: 12,
  lowercase: true,
  uppercase: false,
  digits: true,
  symbols: false,
  customSymbols: null,
  excludeAmbiguous: true,
  excludeChars: null,
  minDigits: 0,
  minSymbols: 0,
};

export const DEFAULT_PASSPHRASE_OPTIONS: PassphraseOptions = {
  words: 5,
  separator: "-",
  capitalize: false,
  includeNumber: false,
  numberPosition: "end",
  wordlist: "eff",
};

export const NUMBER_POSITIONS: { value: NumberPosition; label: string }[] = [
  { value: "end", label: "В конце" },
  { value: "start", label: "В начале" },
];

export const PASSWORD_KINDS: { value: PasswordKind; label: string }[] = [
  { value: "random", label: "Случайный" },
  { value: "readable", label: "Читаемый" },
  { value: "pin", label: "ПИН-код" },
];

export const DEFAULT_PIN_OPTIONS: PinOptions = { length: 6, excludeRepeats: true };
export const PIN_MIN_LENGTH = 4;
export const PIN_MAX_LENGTH = 12;

export const DEFAULT_PRONOUNCEABLE_OPTIONS: PronounceableOptions = {
  length: 16,
  capitalize: false,
};
export const PRONOUNCEABLE_MIN_LENGTH = 8;
export const PRONOUNCEABLE_MAX_LENGTH = 64;

/** Matches `MAX_EXCLUDE_CHARS` in `core/generator.rs`. */
export const MAX_EXCLUDE_CHARS = 64;

export const PASSWORD_MIN_LENGTH = 8;
export const PASSWORD_MAX_LENGTH = 128;
export const LOGIN_MAX_LENGTH = 64;

export const PASSWORD_LENGTH_PRESETS = [8, 12, 16, 20, 32, 64];
export const LOGIN_LENGTH_PRESETS = [8, 12, 16, 24, 32, 48, 64];

export const PASSPHRASE_MIN_WORDS = 3;
export const PASSPHRASE_MAX_WORDS = 12;

export const PASSPHRASE_SEPARATORS: { value: string; label: string }[] = [
  { value: "-", label: "Дефис -" },
  { value: ".", label: "Точка ." },
  { value: "_", label: "Подчёркивание _" },
  { value: " ", label: "Пробел" },
  { value: "+", label: "Плюс +" },
  { value: "=", label: "Знак равенства =" },
  { value: ":", label: "Двоеточие :" },
  { value: "", label: "Без разделителя" },
];

/**
 * Sends an empty field as `null` so the backend keeps its default and the UI
 * does not have to distinguish "" from "nothing excluded".
 */
export function normalizeExcludeChars(value: string): string | null {
  const trimmed = value.replace(/\s+/g, "");
  return trimmed.length > 0 ? trimmed.slice(0, MAX_EXCLUDE_CHARS) : null;
}

export function hasSelectedCharset(options: PasswordOptions): boolean {
  return options.lowercase || options.uppercase || options.digits || options.symbols;
}

/**
 * Keeps the minimums inside what the backend accepts: a minimum only counts
 * for an enabled set, neither exceeds the length, and both together still fit.
 */
export function clampMinimums(options: PasswordOptions): PasswordOptions {
  const length = options.length;
  let minDigits = options.digits ? clampInt(options.minDigits, 0, length) : 0;
  let minSymbols = options.symbols ? clampInt(options.minSymbols, 0, length) : 0;

  if (minDigits + minSymbols > length) {
    if (minDigits >= minSymbols) {
      minDigits = Math.max(0, length - minSymbols);
    } else {
      minSymbols = Math.max(0, length - minDigits);
    }
  }

  return minDigits === options.minDigits && minSymbols === options.minSymbols
    ? options
    : { ...options, minDigits, minSymbols };
}

export function clampWords(words: number): number {
  return clampInt(words, PASSPHRASE_MIN_WORDS, PASSPHRASE_MAX_WORDS);
}

export function clampPinLength(length: number): number {
  return clampInt(length, PIN_MIN_LENGTH, PIN_MAX_LENGTH);
}

export function clampPronounceableLength(length: number): number {
  return clampInt(length, PRONOUNCEABLE_MIN_LENGTH, PRONOUNCEABLE_MAX_LENGTH);
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.round(value)));
}
