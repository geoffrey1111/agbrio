import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,it,expect,vi} from "vitest";
import {AssistantSettings,type AssistantSettingsApi} from "./AssistantSettings";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
function api():AssistantSettingsApi{return {view:vi.fn().mockResolvedValue({mcpUrl:"https://qa.invalid/mcp",grants:[],bridges:[{id:"exact-bridge",name:"QA Bridge",bindings:{bindingRevision:7,decision:{endpoint:{label:"Control",externalId:"source-native"}},execution:{endpoint:{label:"Execution",externalId:"target-native"}}}}]}),create:vi.fn().mockResolvedValue({}),revoke:vi.fn().mockResolvedValue({})};}
it("requires an explicit owner brief for the whole instance, independent of selected Bridge",async()=>{
 setLanguagePreference("zh-CN");const a=api();render(<AssistantSettings api={a}/>);fireEvent.click(await screen.findByRole("button",{name:"添加授权"}));expect(screen.getByRole("button",{name:"授权接管 Agbrio"})).toBeDisabled();
 fireEvent.change(screen.getByLabelText("Brief / Insight"),{target:{value:"Continue the approved plan.\n\nAsk me about budget changes."}});expect(screen.queryByLabelText("交接方向")).toBeNull();expect(screen.queryByLabelText("Bridge")).toBeNull();fireEvent.click(screen.getByRole("button",{name:"授权接管 Agbrio"}));
 await waitFor(()=>expect(a.create).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({label:"Dot",rules:[{id:"rule-1",text:"Continue the approved plan."},{id:"rule-2",text:"Ask me about budget changes."}]})));expect(await screen.findByRole("status")).toHaveTextContent("授权已添加");
});
it("keeps the user's brief after failure and uses English chrome without translating its bytes",async()=>{
 setLanguagePreference("en");const a=api();vi.mocked(a.create).mockRejectedValue(Error("ASSISTANT_GRANT_INVALID"));render(<AssistantSettings api={a}/>);fireEvent.click(await screen.findByRole("button",{name:"Add delegation"}));fireEvent.change(screen.getByLabelText("Brief / Insight"),{target:{value:"保持这个中文 brief"}});fireEvent.click(screen.getByRole("button",{name:"Delegate Agbrio control"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("The operation did not complete");expect(screen.getByLabelText("Brief / Insight")).toHaveValue("保持这个中文 brief");
});

it("allows instance setup before any Bridge exists and identifies old grants without elevating them",async()=>{
 const a=api();vi.mocked(a.view).mockResolvedValue({mcpUrl:null,bridges:[],grants:[{id:"old",scope:"BRIDGE",workstreamId:"old-bridge",sourceRole:"DECISION",label:"Old client",rules:[],expiresAt:Date.now()+86400000,revokedAt:null}]});
 render(<AssistantSettings api={a}/>);expect(await screen.findByText(/旧版单 Bridge 授权/)).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'添加授权'}));expect(screen.getByText('整个 Agbrio · 包含以后新建的 Bridge')).toBeVisible();
 fireEvent.change(screen.getByLabelText('Brief / Insight'),{target:{value:'Follow my current decision brief.'}});fireEvent.click(screen.getByRole('button',{name:'授权接管 Agbrio'}));
 await waitFor(()=>expect(a.create).toHaveBeenCalledOnce());expect(vi.mocked(a.create).mock.calls[0][0]).not.toHaveProperty('workstreamId');
});
