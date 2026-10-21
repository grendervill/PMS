const PREFIX = "pms.biometric.";

function key(vaultId: string) {
  return `${PREFIX}${vaultId}`;
}

export function isBiometricEnabled(vaultId?: string | null): boolean {
  if (!vaultId) return false;
  try {
    return localStorage.getItem(key(vaultId)) === "1";
  } catch {
    return false;
  }
}

export function setBiometricEnabled(vaultId: string, enabled: boolean): void {
  try {
    if (enabled) {
      localStorage.setItem(key(vaultId), "1");
    } else {
      localStorage.removeItem(key(vaultId));
    }
  } catch {
    // localStorage can be unavailable; biometric fallback still works.
  }
}
