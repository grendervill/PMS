export type PasswordOptions = {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  customSymbols?: string | null;
  excludeAmbiguous: boolean;
};

export type LoginOptions = PasswordOptions;

export const defaultPasswordOptions: PasswordOptions = {
  length: 16,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  customSymbols: null,
  excludeAmbiguous: true,
};

export const defaultLoginOptions: PasswordOptions = {
  length: 12,
  lowercase: true,
  uppercase: false,
  digits: true,
  symbols: false,
  customSymbols: null,
  excludeAmbiguous: true,
};

export function hasSelectedCharset(options: PasswordOptions): boolean {
  return options.lowercase || options.uppercase || options.digits || options.symbols;
}
