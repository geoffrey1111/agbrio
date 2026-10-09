import {cleanup,fireEvent,render,screen,within} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {AssistantConnectionGuide} from "./AssistantConnectionGuide";
import {assistantSetupInstructions} from "./assistantSetupInstructions";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
function setup(grants:Parameters<typeof AssistantConnectionGuide>[0]["grants"]=[],url:string|null="https://instance.fixture.invalid/mcp"){
 const onAddGrant=vi.fn(),onRouteChange=vi.fn(),openHelp=vi.fn().mockResolvedValue(undefined);
 render(<AssistantConnectionGuide mcpUrl={url} grants={grants} onAddGrant={onAddGrant} onRouteChange={onRouteChange} openHelp={openHelp}/>);return {onAddGrant,onRouteChange,openHelp};
}
it("separates cloud from local steps without creating authority or declaring a connection",()=>{
 setLanguagePreference("en");const a=setup();fireEvent.click(screen.getByRole("button",{name:/ChatGPT/}));fireEvent.click(screen.getByText("Do it yourself: detailed guide"));fireEvent.click(screen.getByRole("button",{name:/Register cloud plugin/}));expect(screen.getByText("agbrio:instance")).toBeVisible();fireEvent.click(screen.getAllByRole("button",{name:"Open Plugins in browser"}).at(-1)!);expect(a.openHelp).toHaveBeenCalledWith("CHATGPT_PLUGINS");
 fireEvent.click(screen.getByRole("button",{name:/Read-only validation/}));expect(screen.getAllByText("Awaiting actual evidence")).toHaveLength(4);fireEvent.click(screen.getByRole("button",{name:/Codex/}));fireEvent.click(screen.getByRole("button",{name:/Add to Codex/}));expect(screen.queryByRole("button",{name:"Open Plugins in browser"})).toBeNull();expect(a.onAddGrant).not.toHaveBeenCalled();expect(a.onRouteChange).toHaveBeenLastCalledWith("local");
});
it("reuses valid instance grants without upgrading legacy or expired ones",()=>{
 const now=Date.now();const grants=[{id:"old",scope:"BRIDGE",approvalMode:"BRIEF_RULES",label:"Old",expiresAt:now+900000,revokedAt:null},{id:"valid",scope:"INSTANCE",approvalMode:"CONVERSATION_REVIEW",label:"Review assistant",expiresAt:now+900000,revokedAt:null},{id:"expired",scope:"INSTANCE",approvalMode:"CONVERSATION_REVIEW",label:"Expired",expiresAt:now-1,revokedAt:null}];const a=setup(grants);fireEvent.click(screen.getByRole("button",{name:/ChatGPT/}));fireEvent.click(screen.getByText("自己操作：查看详细教程"));fireEvent.click(screen.getByRole("button",{name:/准备授权/}));expect(screen.getByText("Review assistant")).toBeVisible();expect(screen.queryByText("Expired")).toBeNull();expect(screen.queryByRole("button",{name:"创建 30 天授权"})).toBeNull();expect(a.onAddGrant).not.toHaveBeenCalled();
});
it("requires explicit add action and keeps unconfigured URLs unverified",()=>{
 const a=setup([],null);fireEvent.click(screen.getByRole("button",{name:/ChatGPT/}));expect(screen.getAllByText(/尚未配置 MCP/)[0]).toBeVisible();fireEvent.click(screen.getByText("自己操作：查看详细教程"));fireEvent.click(screen.getByRole("button",{name:/准备授权/}));expect(a.onAddGrant).not.toHaveBeenCalled();fireEvent.click(screen.getAllByRole("button",{name:"创建 30 天授权"})[0]);expect(a.onAddGrant).toHaveBeenCalledOnce();
});
it.each(["en","zh-TW"] as const)("renders every guide step in %s with no untranslated Simplified Chinese chrome",language=>{
 setLanguagePreference(language);setup();fireEvent.click(screen.getByRole("button",{name:/ChatGPT/}));fireEvent.click(screen.getByText(language==="en"?"Do it yourself: detailed guide":"自己操作：查看詳細教程"));const section=screen.getByRole("region",{name:language==="en"?"Connect your assistant":"連接你的助手"});
 for(let i=0;i<6;i++){fireEvent.click(within(section).getByRole("navigation").querySelectorAll("button")[i]);if(language==="en")expect(section.textContent).not.toMatch(/[\u3400-\u9fff]/);}
});
it("copies only route-specific instructions and retains exact decision/retry safeguards",()=>{
 const local=assistantSetupInstructions("https://instance.fixture.invalid/mcp","en","local");const cloud=assistantSetupInstructions("https://instance.fixture.invalid/mcp","en","cloud");expect(local).toContain("local Codex");expect(local).not.toContain("ChatGPT Plugins → Add custom MCP server");expect(cloud).toContain("scope agbrio:instance");for(const prompt of [local,cloud]){expect(prompt).toContain("Which Bridges");expect(prompt).toContain("ruleId=null");expect(prompt).toContain("decisionId=null");expect(prompt).toContain("user-message reference");expect(prompt).toContain("requestId");}
});
