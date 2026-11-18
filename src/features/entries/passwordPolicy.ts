import type { PasswordPolicy } from "@/features/vault/types";

/** Matches `MAX_EXCLUDE_CHARS` in the generator; mirrors `forbidden_chars`. */
export const MAX_FORBIDDEN_CHARS = 64;

/**
 * The generator cannot build anything longer than this, so a policy asking for
 * more is capped instead of making the dialog unusable.
 */
export const POLICY_MAX_LENGTH = 128;

export function emptyPasswordPolicy(): PasswordPolicy {
  return {
    minLength: 0,
    maxLength: 0,
    requireLowercase: false,
    requireUppercase: false,
    requireDigits: false,
    requireSymbols: false,
    forbiddenChars: "",
  };
}

/** Empty strings and zeros carry no meaning, so they become `undefined`. */
export function isPasswordPolicyEmpty(policy: PasswordPolicy | null | undefined): boolean {
  if (!policy) return true;
  return (
    policy.minLength === 0 &&
    policy.maxLength === 0 &&
    !policy.requireLowercase &&
    !policy.requireUppercase &&
    !policy.requireDigits &&
    !policy.requireSymbols &&
    policy.forbiddenChars.length === 0
  );
}

/**
 * Turns whatever the form holds into a policy the vault accepts: numbers are
 * clamped, the forbidden list is deduplicated and trimmed, and an all-default
 * policy disappears so the vault file stays byte-identical to the old shape.
 */
export function normalizePasswordPolicy(
  policy: Partial<PasswordPolicy> | null | undefined,
): PasswordPolicy | undefined {
  if (!policy) return undefined;
  const minLength = clampLength(policy.minLength);
  const maxLength = clampLength(policy.maxLength);
  const normalized: PasswordPolicy = {
    minLength: minLength > 0 && maxLength > 0 ? Math.min(minLength, maxLength) : minLength,
    maxLength,
    requireLowercase: Boolean(policy.requireLowercase),
    requireUppercase: Boolean(policy.requireUppercase),
    requireDigits: Boolean(policy.requireDigits),
    requireSymbols: Boolean(policy.requireSymbols),
    forbiddenChars: normalizeForbiddenChars(policy.forbiddenChars),
  };
  return isPasswordPolicyEmpty(normalized) ? undefined : normalized;
}

/** Whitespace never survives in the generator's exclusion list, so it goes. */
export function normalizeForbiddenChars(value: string | null | undefined): string {
  if (!value) return "";
  const seen = new Set<string>();
  let result = "";
  for (const character of value.replace(/\s+/g, "")) {
    if (seen.has(character)) continue;
    seen.add(character);
    result += character;
    if (result.length >= MAX_FORBIDDEN_CHARS) break;
  }
  return result;
}

export function requiredCharSets(policy: PasswordPolicy): string[] {
  const sets: string[] = [];
  if (policy.requireLowercase) sets.push("строчные буквы");
  if (policy.requireUppercase) sets.push("заглавные буквы");
  if (policy.requireDigits) sets.push("цифры");
  if (policy.requireSymbols) sets.push("символы");
  return sets;
}

/** One line the editor, the detail view and the generator can all reuse. */
export function passwordPolicySummary(policy: PasswordPolicy): string {
  const parts: string[] = [];
  if (policy.minLength > 0 && policy.maxLength > 0) {
    parts.push(`длина ${policy.minLength}–${policy.maxLength}`);
  } else if (policy.minLength > 0) {
    parts.push(`не короче ${policy.minLength}`);
  } else if (policy.maxLength > 0) {
    parts.push(`не длиннее ${policy.maxLength}`);
  }

  const sets = requiredCharSets(policy);
  if (sets.length > 0) parts.push(`обязательны: ${sets.join(", ")}`);
  if (policy.forbiddenChars) parts.push(`запрещены ${policy.forbiddenChars}`);

  return parts.join(" · ");
}

/**
 * `false` for a policy of numbers that cannot both hold (min above the
 * generator's ceiling, for instance), so the editor can refuse to save it.
 */
export function passwordPolicyError(policy: PasswordPolicy): string | null {
  if (policy.minLength > POLICY_MAX_LENGTH) {
    return `Минимальная длина не может быть больше ${POLICY_MAX_LENGTH}`;
  }
  if (policy.maxLength > 0 && policy.minLength > policy.maxLength) {
    return "Минимальная длина больше максимальной";
  }
  return null;
}

function clampLength(value: number | undefined): number {
  if (!value || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(POLICY_MAX_LENGTH, Math.round(value)));
}
