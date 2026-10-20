import { useCallback, useRef, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import {
  ConfirmContext,
  type ConfirmFn,
  type ConfirmOptions,
} from "@/components/ui/confirm-context";

type ConfirmRequest = {
  options: ConfirmOptions;
  resolve: (value: boolean) => void;
};

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const requestRef = useRef<ConfirmRequest | null>(null);
  const [request, setRequest] = useState<ConfirmRequest | null>(null);

  const finish = useCallback((value: boolean) => {
    const current = requestRef.current;
    requestRef.current = null;
    setRequest(null);
    current?.resolve(value);
  }, []);

  const confirm = useCallback<ConfirmFn>((options) => {
    requestRef.current?.resolve(false);
    return new Promise<boolean>((resolve) => {
      const next = { options, resolve };
      requestRef.current = next;
      setRequest(next);
    });
  }, []);

  const options = request?.options;

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Modal
        open={request !== null}
        onOpenChange={(open) => {
          if (!open) finish(false);
        }}
        title={options?.title ?? ""}
        description={options?.description}
        className="w-[min(92vw,460px)]"
        footer={
          <>
            <Button variant="ghost" onClick={() => finish(false)}>
              {options?.cancelLabel ?? "Отмена"}
            </Button>
            <Button
              variant={options?.tone === "danger" ? "danger" : "primary"}
              onClick={() => finish(true)}
            >
              {options?.confirmLabel ?? "Продолжить"}
            </Button>
          </>
        }
      >
        <div />
      </Modal>
    </ConfirmContext.Provider>
  );
}
