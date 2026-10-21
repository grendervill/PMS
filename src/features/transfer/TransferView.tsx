import {
  ArrowRight,
  Check,
  Download,
  FolderOpen,
  Loader2,
  LockKeyhole,
  Radar,
  Send,
  ShieldCheck,
  Wifi,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { EmptyState } from "@/components/ui/EmptyState";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { errorText } from "@/features/vault/errors";
import {
  vaultApi,
  type ReceiveStarted,
  type ReceivedArchive,
  type TransferPeer,
} from "@/features/vault/api";
import { useVaultStore } from "@/features/vault/store";
import { getStorage } from "@/lib/storage";
import {
  cleanupCacheDirectory,
  installNativeArchive,
  stageVaultForTransfer,
} from "@/lib/storage/archive";
import { isAndroidPlatform } from "@/lib/platform";
import { cn } from "@/lib/cn";

type Tab = "send" | "receive";
type ReceiveMode = "new" | "replace";

export function TransferView() {
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const lock = useVaultStore((state) => state.lock);
  const openVault = useVaultStore((state) => state.openVault);
  const toast = useToast();
  const confirm = useConfirm();
  const [tab, setTab] = useState<Tab>("send");
  const [peers, setPeers] = useState<TransferPeer[]>([]);
  const [selectedPeer, setSelectedPeer] = useState<TransferPeer | null>(null);
  const [manualHost, setManualHost] = useState("");
  const [manualPort, setManualPort] = useState("");
  const [code, setCode] = useState("");
  const [scanning, setScanning] = useState(false);
  const [sending, setSending] = useState(false);
  const [receiveMode, setReceiveMode] = useState<ReceiveMode>("new");
  const [destination, setDestination] = useState("");
  const [receiveStarted, setReceiveStarted] = useState<ReceiveStarted | null>(null);
  const [receiving, setReceiving] = useState(false);
  const [received, setReceived] = useState<ReceivedArchive | null>(null);
  const [error, setError] = useState<string | null>(null);

  const scan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const found = await vaultApi.transferScan(4000);
      setPeers(found);
      if (found.length === 0) {
        setSelectedPeer(null);
      }
    } catch (scanError) {
      setError(errorText(scanError));
    } finally {
      setScanning(false);
    }
  }, []);

  const installReceived = useCallback(
    async (archive: ReceivedArchive) => {
      setReceiving(true);
      setError(null);
      try {
        if (archive.replaceCurrent) {
          await lock();
        }
        const installed = isAndroidPlatform()
          ? await installNativeArchive(
              archive.archivePath,
              archive.destination,
              archive.replaceCurrent,
            )
          : await vaultApi.transferInstall(
              archive.archivePath,
              archive.destination,
              archive.replaceCurrent,
            );
        setReceived(null);
        setReceiveStarted(null);
        toast.show({
          title: archive.replaceCurrent ? "Хранилище заменено" : "Хранилище принято",
          description: installed,
          tone: "success",
        });
        await openVault(installed);
      } catch (installError) {
        setError(errorText(installError));
        setReceived(null);
        setReceiveStarted(null);
      } finally {
        setReceiving(false);
      }
    },
    [lock, openVault, toast],
  );

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let active = true;
    void listen<ReceivedArchive>("transfer://received", (event) => {
      if (!active) return;
      setReceived(event.payload);
      void installReceived(event.payload);
    })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
      void vaultApi.transferReceiveStop().catch(() => undefined);
    };
  }, [installReceived]);

  const send = async () => {
    if (!selectedPeer) {
      setError("Выберите устройство.");
      return;
    }
    if (!/^\d{6}$/.test(code.trim())) {
      setError("Введите шестизначный код с принимающего устройства.");
      return;
    }
    if (!vaultPath) {
      setError("Хранилище не открыто.");
      return;
    }
    setSending(true);
    setError(null);
    let staged: string | null = null;
    try {
      const source = isAndroidPlatform()
        ? (staged = await stageVaultForTransfer(vaultPath))
        : vaultPath;
      await vaultApi.transferSend(selectedPeer.host, selectedPeer.port, code.trim(), source);
      toast.show({
        title: "Хранилище отправлено",
        description: selectedPeer.name,
        tone: "success",
      });
    } catch (sendError) {
      setError(errorText(sendError));
    } finally {
      if (staged) await cleanupCacheDirectory(staged);
      setSending(false);
    }
  };

  const selectManualPeer = () => {
    const host = manualHost.trim();
    const port = Number(manualPort);
    if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
      setError("Введите IP-адрес и порт с принимающего устройства.");
      return;
    }
    setSelectedPeer({
      id: `manual:${host}:${port}`,
      name: `Вручную · ${host}`,
      host,
      port,
    });
    setError(null);
  };

  const chooseDestination = async () => {
    const selected = await getStorage().pickFolder();
    if (selected) {
      setDestination(selected);
      setError(null);
    }
  };

  const startReceive = async () => {
    if (!vaultPath) return;
    const replaceCurrent = receiveMode === "replace";
    const target = replaceCurrent ? vaultPath : destination;
    if (!target) {
      setError("Выберите папку для нового хранилища.");
      return;
    }
    if (replaceCurrent) {
      const allowed = await confirm({
        title: "Заменить текущее хранилище?",
        description:
          "После передачи текущее хранилище будет заменено. Перед заменой будет создана резервная копия.",
        confirmLabel: "Продолжить",
        tone: "danger",
      });
      if (!allowed) return;
    }
    setReceiving(true);
    setError(null);
    setReceived(null);
    try {
      setReceiveStarted(await vaultApi.transferReceiveStart(target, replaceCurrent));
    } catch (startError) {
      setError(errorText(startError));
    } finally {
      setReceiving(false);
    }
  };

  const stopReceive = async () => {
    await vaultApi.transferReceiveStop().catch(() => undefined);
    setReceiveStarted(null);
    setReceived(null);
  };

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-5xl">
        <div
          role="tablist"
          aria-label="Передача хранилища"
          className="mb-5 grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1"
        >
          {(
            [
              ["send", "Передать на", Send],
              ["receive", "Принять с", Download],
            ] as const
          ).map(([value, label, Icon]) => (
            <button
              key={value}
              type="button"
              role="tab"
              aria-selected={tab === value}
              onClick={() => {
                if (value === "send" && receiveStarted) {
                  void stopReceive();
                }
                setTab(value);
                setError(null);
              }}
              className={cn(
                "flex h-9 items-center justify-center gap-2 rounded-[6px] text-[12px] transition-colors",
                tab === value
                  ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                  : "text-[var(--muted)] hover:text-[var(--text-soft)]",
              )}
            >
              <Icon size={14} />
              {label}
            </button>
          ))}
        </div>

        {error ? (
          <p
            role="alert"
            className="mb-4 rounded-[var(--radius-sm)] border border-[var(--danger)]/25 bg-[var(--danger)]/8 px-3 py-2 text-[11.5px] text-[var(--danger)]"
          >
            {error}
          </p>
        ) : null}

        {tab === "send" ? (
          <div role="tabpanel" aria-label="Передать на" className="grid gap-4">
            <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h2 className="pms-display text-lg">Устройства рядом</h2>
                  <p className="mt-1 text-[11px] text-[var(--muted)]">
                    Видны только устройства с запущенным PMS в локальной сети.
                  </p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void scan()}
                  disabled={scanning}
                >
                  {scanning ? <Loader2 size={14} className="animate-spin" /> : <Radar size={14} />}
                  Найти
                </Button>
              </div>

              {peers.length > 0 ? (
                <div className="mt-3 grid gap-2">
                  {peers.map((peer) => (
                    <button
                      key={peer.id}
                      type="button"
                      aria-pressed={selectedPeer?.id === peer.id}
                      onClick={() => setSelectedPeer(peer)}
                      className={cn(
                        "flex items-center gap-3 rounded-[var(--radius-sm)] border px-3 py-2.5 text-left transition-colors",
                        selectedPeer?.id === peer.id
                          ? "border-[var(--accent)]/50 bg-[var(--accent)]/8"
                          : "border-[var(--border)] bg-[var(--surface-2)]/50 hover:border-[var(--border-strong)]",
                      )}
                    >
                      <span className="grid size-8 place-items-center rounded-[8px] border border-[var(--border)] text-[var(--accent-soft)]">
                        <Wifi size={15} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[12.5px] text-[var(--text-soft)]">
                          {peer.name}
                        </span>
                        <span className="pms-mono block text-[10px] text-[var(--muted)]">
                          {peer.host}:{peer.port}
                        </span>
                      </span>
                      {selectedPeer?.id === peer.id ? (
                        <Check size={15} className="shrink-0 text-[var(--accent)]" />
                      ) : null}
                    </button>
                  ))}
                </div>
              ) : (
                <div className="mt-3">
                  <EmptyState
                    icon={Radar}
                    title={scanning ? "Ищем устройства" : "Пока никого не видно"}
                    description="Откройте «Принять с» на другом устройстве PMS и нажмите «Ждать передачу»."
                    className="min-h-[180px]"
                  />
                </div>
              )}

              <div className="mt-3 rounded-[var(--radius-md)] border border-dashed border-[var(--border)] bg-[var(--surface-2)]/30 p-3">
                <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
                  Не видно устройство? Введите IP и порт с экрана «Принять с».
                </p>
                <div className="mt-2 grid grid-cols-[1fr_84px] gap-2">
                  <Input
                    aria-label="IP-адрес устройства"
                    value={manualHost}
                    onChange={(event) => setManualHost(event.target.value)}
                    placeholder="192.168.1.10"
                    className="pms-mono text-[12px]"
                  />
                  <Input
                    aria-label="Порт устройства"
                    value={manualPort}
                    onChange={(event) =>
                      setManualPort(event.target.value.replace(/\D/g, "").slice(0, 5))
                    }
                    placeholder="4040"
                    inputMode="numeric"
                    className="pms-mono text-[12px]"
                  />
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  className="mt-2"
                  onClick={selectManualPeer}
                >
                  Выбрать вручную
                </Button>
                {selectedPeer?.id.startsWith("manual:") ? (
                  <p className="mt-2 truncate text-[10.5px] text-[var(--accent-soft)]">
                    Выбрано: {selectedPeer.host}:{selectedPeer.port}
                  </p>
                ) : null}
              </div>
            </section>

            <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-4">
              <h2 className="pms-display text-lg">Подтверждение</h2>
              <p className="mt-1 text-[11px] leading-relaxed text-[var(--muted)]">
                Введите код, который показан на принимающем устройстве. Он подтверждает, что вы
                передаёте хранилище именно туда.
              </p>
              <div className="mt-3 flex flex-col gap-3 sm:flex-row sm:items-end">
                <Field label="Код с другого устройства" htmlFor="transfer-code">
                  <Input
                    id="transfer-code"
                    value={code}
                    onChange={(event) => {
                      setCode(event.target.value.replace(/\D/g, "").slice(0, 6));
                      setError(null);
                    }}
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    placeholder="000000"
                    className="pms-mono tracking-[0.25em]"
                  />
                </Field>
                <Button onClick={() => void send()} disabled={sending || !selectedPeer}>
                  {sending ? <Loader2 size={15} className="animate-spin" /> : <Send size={15} />}
                  Передать
                </Button>
              </div>
            </section>

            <p className="text-[10px] leading-relaxed text-[var(--muted)]/75">
              Передаётся полная зашифрованная копия: pms.vault и вложения. Парольная фраза и
              ключ-файл не передаются.
            </p>
          </div>
        ) : (
          <div role="tabpanel" aria-label="Принять с" className="grid gap-4">
            <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-4">
              <h2 className="pms-display text-lg">Куда принять</h2>
              <p className="mt-1 text-[11px] text-[var(--muted)]">
                Выберите до начала передачи. Текущее хранилище можно заменить или сохранить
                полученное в новую папку.
              </p>
              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                <button
                  type="button"
                  aria-pressed={receiveMode === "new"}
                  onClick={() => setReceiveMode("new")}
                  className={cn(
                    "rounded-[var(--radius-md)] border p-3 text-left transition-colors",
                    receiveMode === "new"
                      ? "border-[var(--accent)]/50 bg-[var(--accent)]/8"
                      : "border-[var(--border)] bg-[var(--surface-2)]/50 hover:border-[var(--border-strong)]",
                  )}
                >
                  <span className="flex items-center gap-2 text-[12px] font-medium text-[var(--text-soft)]">
                    <FolderOpen size={14} className="text-[var(--accent-soft)]" />
                    Выбрать новую папку
                  </span>
                  <span className="mt-1 block text-[10.5px] leading-relaxed text-[var(--muted)]">
                    Полученное хранилище будет открываться отдельно.
                  </span>
                </button>
                <button
                  type="button"
                  aria-pressed={receiveMode === "replace"}
                  onClick={() => setReceiveMode("replace")}
                  className={cn(
                    "rounded-[var(--radius-md)] border p-3 text-left transition-colors",
                    receiveMode === "replace"
                      ? "border-[var(--danger)]/50 bg-[var(--danger)]/8"
                      : "border-[var(--border)] bg-[var(--surface-2)]/50 hover:border-[var(--border-strong)]",
                  )}
                >
                  <span className="flex items-center gap-2 text-[12px] font-medium text-[var(--text-soft)]">
                    <LockKeyhole size={14} className="text-[var(--warning)]" />
                    Заменить текущее
                  </span>
                  <span className="mt-1 block text-[10.5px] leading-relaxed text-[var(--muted)]">
                    Текущее хранилище будет заменено после резервной копии.
                  </span>
                </button>
              </div>

              {receiveMode === "new" ? (
                <div className="mt-3 flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/50 px-3 py-2">
                  <span className="min-w-0 flex-1 truncate pms-mono text-[10.5px] text-[var(--muted)]">
                    {destination ? getStorage().displayName(destination) : "Папка не выбрана"}
                  </span>
                  <Button variant="secondary" size="sm" onClick={() => void chooseDestination()}>
                    <FolderOpen size={14} />
                    Выбрать
                  </Button>
                </div>
              ) : null}

              <div className="mt-3 flex flex-wrap gap-2">
                {!receiveStarted ? (
                  <Button onClick={() => void startReceive()} disabled={receiving}>
                    {receiving ? (
                      <Loader2 size={15} className="animate-spin" />
                    ) : (
                      <ArrowRight size={15} />
                    )}
                    Ждать передачу
                  </Button>
                ) : (
                  <Button variant="secondary" onClick={() => void stopReceive()}>
                    Остановить приём
                  </Button>
                )}
              </div>
            </section>

            {receiveStarted ? (
              <section className="rounded-[var(--radius-lg)] border border-[var(--accent)]/30 bg-[var(--accent)]/6 p-5 text-center">
                <p className="text-[10.5px] font-medium tracking-wide text-[var(--accent-soft)]">
                  КОД ПОДТВЕРЖДЕНИЯ
                </p>
                <p className="pms-mono mt-2 text-[38px] tracking-[0.25em] text-[var(--text)]">
                  {receiveStarted.code}
                </p>
                <div className="mt-1 flex items-center justify-center gap-1.5 text-[10.5px] text-[var(--muted)]">
                  <span>Введите его на отправляющем устройстве</span>
                  <CopyButton value={receiveStarted.code} label="Копировать код" />
                </div>
                <div className="mt-4 flex items-center justify-center gap-2 text-[11px] text-[var(--text-soft)]">
                  {receiving ? (
                    <>
                      <Loader2 size={14} className="animate-spin text-[var(--accent-soft)]" />
                      Принимаем данные…
                    </>
                  ) : (
                    <>
                      <Wifi size={14} className="text-[var(--accent-soft)]" />
                      Ожидаем устройство PMS…
                    </>
                  )}
                </div>
                <div className="mt-3 flex items-center justify-center gap-1.5 text-[10px] text-[var(--muted)]">
                  <span>
                    {receiveStarted.deviceName} · {receiveStarted.host} · порт{" "}
                    {receiveStarted.port}
                  </span>
                  <CopyButton
                    value={`${receiveStarted.host}:${receiveStarted.port}`}
                    label="Копировать адрес"
                  />
                </div>
              </section>
            ) : null}

            {received ? (
              <section className="rounded-[var(--radius-md)] border border-[var(--accent)]/30 bg-[var(--accent)]/8 p-4 text-[11.5px] text-[var(--text-soft)]">
                Получено от {received.sender}. Устанавливаем…
              </section>
            ) : null}

            <div className="flex items-start gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/50 px-3 py-2.5 text-[10px] leading-relaxed text-[var(--muted)]">
              <ShieldCheck size={13} className="mt-0.5 shrink-0 text-[var(--accent-soft)]" />
              Принимающее устройство должно быть разблокировано. После замены старый vault
              сохраняется в backups/.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
