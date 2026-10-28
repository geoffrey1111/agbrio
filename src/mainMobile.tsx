import {registerCurrentServiceWorker} from "./mobile/push";
import {startPwaUpdates} from './mobile/pwaUpdates';
import { WebLoginGate } from "./mobile/WebLogin";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/unified-workbench-v3.css";
import "./styles/warm-workbench.css";
import "./styles/web-login.css";
import "./styles/watch-chat.css";
import { UnifiedMobileWorkbenchHost } from "./mobile/UnifiedMobileWorkbenchHost";

if(window.isSecureContext&&"serviceWorker" in navigator){void registerCurrentServiceWorker().then(startPwaUpdates).catch(()=>undefined);}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <WebLoginGate><UnifiedMobileWorkbenchHost initialSurface={location.pathname==="/mobile/notifications"?"NOTIFICATIONS":"BRIDGES"}/></WebLoginGate>
  </StrictMode>,
);

import "./styles/bridge-v5.css";
import "./styles/native-router.css";

import "./styles/interaction-motion.css";
import "./styles/conversation-v6.css";
import "./styles/agbrio-v6-theme.css";
import "./styles/mobile-inputs.css";
import "./styles/conversation-polish.css";
