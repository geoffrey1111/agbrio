import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,it,expect,vi} from "vitest";
import {AssistantSettings,type AssistantSettingsApi} from "./AssistantSettings";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
function api():AssistantSettingsApi{return {view:vi.fn().mockResolvedValue({mcpUrl:"https://qa.invalid/mcp",grants:[],bridges:[]}),create:vi.fn().mockResolvedValue({}),revoke:vi.fn().mockResolvedValue({})};}
it("connects the whole instance by explicit owner action without requiring a business brief",async()=>{
 setLanguagePreference("zh-CN");const a=api();render(<AssistantSettings api={a}/>);expect(a.create).not.toHaveBeenCalled();fireEvent.click(await screen.findByRole("button",{name:"添加助手"}));
 expect(screen.queryByLabelText("Brief / Insight")).toBeNull();expect(screen.queryByLabelText("Bridge")).toBeNull();expect(screen.getByRole("button",{name:"连接助手"})).toBeEnabled();fireEvent.click(screen.getByRole("button",{name:"连接助手"}));
 await waitFor(()=>expect(a.create).toHaveBeenCalledExactlyOnceWith({label:"Dot",expiresAt:expect.any(Number)}));expect(await screen.findByRole("status")).toHaveTextContent("授权已添加");
});
it("keeps the assistant name after failure and uses English chrome",async()=>{
 setLanguagePreference("en");const a=api();vi.mocked(a.create).mockRejectedValue(Error("ASSISTANT_GRANT_INVALID"));render(<AssistantSettings api={a}/>);fireEvent.click(await screen.findByRole("button",{name:"Add assistant"}));fireEvent.change(screen.getByLabelText("Assistant name"),{target:{value:"我的 Dot"}});fireEvent.click(screen.getByRole("button",{name:"Connect assistant"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("The operation did not complete");expect(screen.getByLabelText("Assistant name")).toHaveValue("我的 Dot");
});
it("identifies old grants without elevating them and allows explicit revocation in Traditional Chinese",async()=>{
 setLanguagePreference("zh-TW");const a=api();vi.mocked(a.view).mockResolvedValue({mcpUrl:null,bridges:[],grants:[{id:"old",scope:"BRIDGE",workstreamId:"old-bridge",sourceRole:"DECISION",label:"Old client",rules:[],expiresAt:Date.now()+86400000,revokedAt:null}]});render(<AssistantSettings api={a}/>);
 expect(await screen.findByText(/舊版單 Bridge/)).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'撤銷 Old client 的授權'}));await waitFor(()=>expect(a.revoke).toHaveBeenCalledExactlyOnceWith('old'));
 fireEvent.click(screen.getByRole('button',{name:'新增助手'}));expect(screen.getByText('整個 Agbrio · 包含以後新建的 Bridge')).toBeVisible();fireEvent.change(screen.getByLabelText('助手名稱'),{target:{value:' '}});expect(screen.getByRole('button',{name:'連接助手'})).toBeDisabled();
});
