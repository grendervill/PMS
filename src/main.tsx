import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/app/App";
import { SyncPreview } from "@/dev/SyncPreview";
import { applyTheme, getSelectedTheme } from "@/features/settings/theme";
import "@/styles/global.css";

applyTheme(getSelectedTheme());

// Витрина серверных экранов: только в dev-сборке и только по явному адресу.
const preview =
  import.meta.env.DEV && new URLSearchParams(window.location.search).get("preview") === "sync";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{preview ? <SyncPreview /> : <App />}</React.StrictMode>,
);
