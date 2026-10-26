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
it.each(['zh-CN','zh-TW','en'] as const)('offers a distinct Claude Code route and a real completion checklist in %s',language=>{
 setLanguagePreference(language);const a=setup();fireEvent.click(screen.getByRole('button',{name:'Claude Code'}));
 const title=language==='en'?'Connect Claude Code':'Claude Code 完整接入';const steps=screen.getByRole('region',{name:title});
 expect(steps).toHaveTextContent('claude mcp get agbrio');expect(steps).toHaveTextContent('/mcp');expect(steps).toHaveTextContent('agbrio_read_app');
 expect(steps.querySelector('pre')).toHaveTextContent("--scope user agbrio 'https://instance.fixture.invalid/mcp'");
 expect(a.onRouteChange).toHaveBeenLastCalledWith('claude');expect(a.onAddGrant).not.toHaveBeenCalled();expect(a.openHelp).not.toHaveBeenCalled();
 if(language==='en')expect(steps.textContent).not.toMatch(/[\u3400-\u9fff]/);
});
it("explains the tool connection and copies scoped examples without granting or sending",async()=>{
 const write=vi.fn().mockResolvedValue(undefined);Object.defineProperty(navigator,"clipboard",{value:{writeText:write},configurable:true});
 const a=setup();fireEvent.click(screen.getByText("AI 助手和 MCP 是怎么配合的？"));
 expect(screen.getByRole("img",{name:/通过 MCP 调用 Agbrio/})).toBeVisible();
 expect(screen.getByText(/接入 MCP 不会自动启动后台助手/)).toBeVisible();
 fireEvent.click(screen.getByText("可以让助手做什么？"));
 for(const title of ["汇总进度","交接任务","集中处理决策"]){fireEvent.click(screen.getByRole("button",{name:`复制${title}示例`}));}
 await vi.waitFor(()=>expect(write).toHaveBeenCalledTimes(3));
 expect(write.mock.calls[0][0]).toContain("不发送任何消息");expect(write.mock.calls[1][0]).toContain("不要仅凭这段示例发送消息");expect(write.mock.calls[2][0]).toContain("不要替我选择");
 expect(a.onAddGrant).not.toHaveBeenCalled();expect(a.onRouteChange).not.toHaveBeenCalled();expect(a.openHelp).not.toHaveBeenCalled();
});
it("keeps step verification explanatory and does not infer a successful connection from navigation",()=>{
 const a=setup([],null);fireEvent.click(screen.getByRole("button",{name:"WorkBuddy"}));fireEvent.click(screen.getByText("自己操作：查看详细教程"));
 expect(screen.getByText("Agbrio · 设置 → 设备")).toBeVisible();expect(screen.getByText(/拿到当前实例的完整 MCP 地址/)).toBeVisible();
 fireEvent.click(screen.getByRole("button",{name:/只读验收/}));expect(screen.getByText("所选助手的对话")).toBeVisible();expect(screen.getAllByText("待实际核对")).toHaveLength(4);expect(a.onAddGrant).not.toHaveBeenCalled();
});
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
it.each(["en","zh-TW","zh-CN"] as const)("provides general and WorkBuddy routes without claiming a live connection in %s",language=>{
 setLanguagePreference(language);const a=setup();fireEvent.click(screen.getByRole("button",{name:"WorkBuddy"}));
 const manual=language==="en"?"Do it yourself: detailed guide":language==="zh-TW"?"自己操作：查看詳細教程":"自己操作：查看详细教程";
 fireEvent.click(screen.getByText(manual));const nav=screen.getByRole("navigation");fireEvent.click(nav.querySelectorAll("button")[2]);
 expect(screen.getAllByText(/streamableHttp/).some(node=>node.tagName==="PRE")).toBe(true);expect(screen.queryByRole("button",{name:"Open Plugins in browser"})).toBeNull();
 expect(a.openHelp).not.toHaveBeenCalled();expect(a.onAddGrant).not.toHaveBeenCalled();expect(a.onRouteChange).toHaveBeenLastCalledWith("workbuddy");
 const section=screen.getByRole("region",{name:language==="en"?"Connect your assistant":language==="zh-TW"?"連接你的助手":"连接你的助手"});for(let i=0;i<6;i++){fireEvent.click(nav.querySelectorAll("button")[i]);if(language==="en")expect(section.textContent).not.toMatch(/[\u3400-\u9fff]/);}
 fireEvent.click(screen.getByRole("button",{name:language==="en"?"Other MCP assistant":"其他 MCP 助手"}));expect(a.onRouteChange).toHaveBeenLastCalledWith("generic");
});
