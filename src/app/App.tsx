import { ToastProvider } from "@/components/ui/Toast";
import { AppShell } from "@/app/AppShell";

export default function App() {
  return (
    <ToastProvider>
      <AppShell />
    </ToastProvider>
  );
}
