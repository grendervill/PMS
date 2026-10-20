import { useEffect } from "react";
import { ToastProvider } from "@/components/ui/Toast";
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

  return (
    <ToastProvider>
      {status === "unlocked" ? (
        <AppShell />
      ) : status === "welcome" ? (
        <WelcomeScreen />
      ) : (
        <UnlockScreen />
      )}
    </ToastProvider>
  );
}
