import type { TotpConfig } from "@/features/vault/types";

export function totpUri(config: TotpConfig): string {
  const label = [config.issuer, config.account].filter(Boolean).join(":");
  const params = new URLSearchParams({
    secret: config.secret,
    algorithm: config.algorithm,
    digits: String(config.digits),
    period: String(config.period),
  });
  if (config.issuer) params.set("issuer", config.issuer);
  return `otpauth://totp/${encodeURIComponent(label || "PMS")}?${params.toString()}`;
}
