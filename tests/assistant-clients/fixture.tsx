import {createRoot} from "react-dom/client";
import {AssistantConnectionGuide} from "../../src/features/workbench/AssistantConnectionGuide";
import {setLanguagePreference} from "../../src/i18n";
import "../../src/styles/unified-workbench-v3.css";
import "../../src/styles/warm-workbench.css";
import "../../src/styles/native-router.css";
setLanguagePreference(new URLSearchParams(location.search).get("lang")==="en"?"en":"zh-CN");
createRoot(document.getElementById("root")!).render(<main className="r2-router"><div className="r2-settings" style={{padding:20,maxWidth:800,margin:"auto"}}><div className="r2-assistant-settings"><h1>Agbrio · 隔离连接检查</h1><AssistantConnectionGuide mcpUrl="https://instance.fixture.invalid/mcp" grants={[]} onAddGrant={()=>{}} onRouteChange={()=>{}} openHelp={async()=>{}}/></div></div></main>);
