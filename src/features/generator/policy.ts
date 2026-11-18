import {
  PASSWORD_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
  type PasswordOptions,
} from "@/features/generator/types";
import { passwordPolicySummary } from "@/features/entries/passwordPolicy";
import type { PasswordPolicy } from "@/features/vault/types";

export type PolicyLengthBounds = { min: number; max: number };

/**
 * The length range the dialog allows. The site's own limits win, but they are
 * pulled inside what the backend can actually generate (8..128) so that a
 * nonsensical policy cannot lock the slider.
 */
export function policyLengthBounds(policy: PasswordPolicy | null | undefined): PolicyLengthBounds {
  const rawMin = clampLength(policy?.minLength);
  const rawMax = clampLength(policy?.maxLength);
  if (rawMin === 0 && rawMax === 0) {
    return { min: PASSWORD_MIN_LENGTH, max: PASSWORD_MAX_LENGTH };
  }
  const min = rawMin === 0 ? PASSWORD_MIN_LENGTH : rawMin;
  const max = rawMax === 0 ? PASSWORD_MAX_LENGTH : Math.max(rawMax, min);
  return { min, max };
}

/**
 * Applies the site rules to a set of generator options: the length moves into
 * the allowed range, required classes are switched on, and the forbidden
 * characters become the exclusion list.
 */
export function optionsWithPolicy(
  base: PasswordOptions,
  policy: PasswordPolicy | null | undefined,
): PasswordOptions {
  if (!policy) return base;

  const bounds = policyLengthBounds(policy);
  const length = clampInt(base.length, bounds.min, bounds.max);
  const lowercase = base.lowercase || policy.requireLowercase;
  const uppercase = base.uppercase || policy.requireUppercase;
  const digits = base.digits || policy.requireDigits;
  const symbols = base.symbols || policy.requireSymbols;

  return {
    ...base,
    length,
    lowercase,
    uppercase,
    digits,
    symbols,
    // A required class must survive the minimum-count check as well, otherwise
    // the result could pass without it.
    minDigits: digits && policy.requireDigits ? Math.max(1, base.minDigits) : base.minDigits,
    minSymbols: symbols && policy.requireSymbols ? Math.max(1, base.minSymbols) : base.minSymbols,
    excludeChars: policy.forbiddenChars || base.excludeChars || null,
  };
}

/** The line shown above the generator when a site policy is in force. */
export function policyNote(policy: PasswordPolicy | null | undefined): string | null {
  if (!policy) return null;
  const summary = passwordPolicySummary(policy);
  return summary ? `Учтены правила сайта: ${summary}` : null;
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.round(value)));
}

/** A site limit outside the generator's own range is pulled inside it. */
function clampLength(value: number | undefined): number {
  if (!value || !Number.isFinite(value)) return 0;
  return clampInt(value, PASSWORD_MIN_LENGTH, PASSWORD_MAX_LENGTH);
}
