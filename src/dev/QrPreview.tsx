import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { ToastProvider } from "@/components/ui/Toast";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { QrScannerDialog } from "@/features/qr/QrScannerDialog";
import { TotpQrDialog } from "@/features/totp/TotpQrDialog";

/**
 * Витрина QR-экранов для визуальной проверки (только dev-сборка):
 * `?preview=qr` — сканер с камерой и показ TOTP-кода.
 */
export function QrPreview() {
  const [scanner, setScanner] = useState(false);
  const [totp, setTotp] = useState(false);

  return (
    <ToastProvider>
      <ConfirmProvider>
        <main className="grid min-h-screen place-items-center gap-3 bg-[var(--bg)] px-6 text-[var(--text)]">
          <Button onClick={() => setScanner(true)}>Открыть сканер</Button>
          <Button variant="secondary" onClick={() => setTotp(true)}>
            Показать TOTP QR
          </Button>

          <QrScannerDialog
            open={scanner}
            onOpenChange={setScanner}
            onResult={() => setScanner(false)}
          />

          {totp ? (
            <TotpQrDialog
              config={{
                secret: "JBSWY3DPEHPK3PXP",
                digits: 6,
                period: 30,
                algorithm: "SHA1",
                issuer: "PMS",
                account: "alice@example.com",
              }}
              onClose={() => setTotp(false)}
            />
          ) : null}
        </main>
      </ConfirmProvider>
    </ToastProvider>
  );
}
