import { useEffect } from "react";
import { vaultApi } from "@/features/vault/api";
import { getSelectedAppIcon } from "@/features/settings/appIcon";
import { ToastProvider } from "@/components/ui/Toast";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { AppShell } from "@/app/AppShell";
import { useVaultStore } from "@/features/vault/store";
import { WelcomeScreen } from "@/features/vault/WelcomeScreen";
import { UnlockScreen } from "@/features/vault/UnlockScreen";

export default function App() {
  const status = useVaultStore((state) => state.status);
  const init = useVaultStore((state) => state.init);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    void vaultApi.setAppIcon(getSelectedAppIcon()).catch(() => undefined);
  }, []);

  return (
    <ToastProvider>
      <ConfirmProvider>
        {status === "unlocked" ? (
          <AppShell />
        ) : status === "welcome" ? (
          <WelcomeScreen />
        ) : (
          <UnlockScreen />
        )}
      </ConfirmProvider>
    </ToastProvider>
  );
}
