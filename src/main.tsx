import {registerCurrentServiceWorker} from "./mobile/push";
import {startPwaUpdates} from './mobile/pwaUpdates';
import {PwaUpdateNotice} from './mobile/PwaUpdateNotice';
import { WebLoginGate } from "./mobile/WebLogin";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/unified-workbench-v3.css";
import "allotment/dist/style.css";
import "./styles/warm-workbench.css";
import "./styles/web-login.css";
import "./styles/watch-chat.css";
import { UnifiedWorkbenchHost } from "./app/UnifiedWorkbenchHost";
import { UnifiedMobileWorkbenchHost } from "./mobile/UnifiedMobileWorkbenchHost";

if(location.pathname.startsWith("/mobile")&&window.isSecureContext&&"serviceWorker" in navigator){void registerCurrentServiceWorker().then(startPwaUpdates).catch(()=>undefined);}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {location.pathname.startsWith("/mobile") ? <><WebLoginGate><UnifiedMobileWorkbenchHost initialSurface={location.pathname==="/mobile/notifications"?"NOTIFICATIONS":"BRIDGES"}/></WebLoginGate><PwaUpdateNotice/></> : <UnifiedWorkbenchHost />}
  </StrictMode>,
);

import "./styles/bridge-v5.css";
import "./styles/native-router.css";
