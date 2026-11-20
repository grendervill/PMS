import { useState } from "react";
import { ServerConnectScreen } from "./ServerConnectScreen";
import { ServerSetupScreen } from "./ServerSetupScreen";
import { ServerProfileForm } from "./ServerProfileForm";
import type { ServerProfile } from "./profiles";

type Step = "choice" | "setup" | "connect";

/**
 * Серверный режим: сначала выбор «сервер уже есть» или «развернуть», затем
 * форма подключения. Настройку можно пропустить — это и есть «у меня есть».
 */
export function ServerConnectFlow({
  onConnected,
  onCancel,
}: {
  onConnected: (profile: ServerProfile, cacheDir: string) => void;
  onCancel: () => void;
}) {
  const [step, setStep] = useState<Step>("choice");

  if (step === "choice") {
    return (
      <ServerConnectScreen
        onExisting={() => setStep("connect")}
        onSetup={() => setStep("setup")}
        onCancel={onCancel}
      />
    );
  }

  if (step === "setup") {
    return (
      <ServerSetupScreen
        onContinue={() => setStep("connect")}
        onCancel={() => setStep("connect")}
      />
    );
  }

  return <ServerProfileForm onConnected={onConnected} onCancel={() => setStep("choice")} />;
}
