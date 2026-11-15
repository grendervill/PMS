import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/app/App";
import { applyTheme, getSelectedTheme } from "@/features/settings/theme";
import "@/styles/global.css";

applyTheme(getSelectedTheme());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
