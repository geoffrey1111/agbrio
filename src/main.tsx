import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/unified-workbench-v3.css";
import "allotment/dist/style.css";
import "./styles/warm-workbench.css";
import "./styles/web-login.css";
import "./styles/watch-chat.css";
import { UnifiedWorkbenchHost } from "./app/UnifiedWorkbenchHost";

// Desktop entry only. The phone loads `mainMobile` so it never downloads the
// desktop Host, Tauri bindings or the split-pane layout.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <UnifiedWorkbenchHost />
  </StrictMode>,
);

import "./styles/bridge-v5.css";
import "./styles/native-router.css";

import "./styles/interaction-motion.css";
import "./styles/conversation-v6.css";
import "./styles/agbrio-v6-theme.css";
