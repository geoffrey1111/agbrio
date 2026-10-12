import {render,screen,waitFor,cleanup} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {afterEach,expect,it,vi} from "vitest";
import {ClaudeSessionPicker,type ClaudePickerApi} from "./ClaudeSessionPicker";
afterEach(cleanup);
const live=(sessionId:string,title:string,cwd="D:\\fictional")=>({sessionId,title,cwd,version:"2.1.295",state:"COMPLETE",checkedAt:Date.now(),leaseUntil:Date.now()+45000});
it("shows names before IDs, selects exact native identity and rejects stale rows",async()=>{
 const sessions=[live("live-native-id","对话标题"),{...live("stale-native-id","旧对话","D:\\old"),leaseUntil:1}];
 const api:ClaudePickerApi={claudeSessions:vi.fn().mockResolvedValue({sessions,complete:true,writesEnabled:false}),claudeSetup:vi.fn(),claudeConnect:vi.fn()};const change=vi.fn();
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={change}/>);
 const choice=await screen.findByRole("radio",{name:/对话标题/});expect(choice).not.toHaveTextContent("live-native-id");
 expect(api.claudeSetup).not.toHaveBeenCalled();expect(api.claudeConnect).not.toHaveBeenCalled();
 expect(screen.getByRole("radio",{name:/旧对话/})).toBeDisabled();
 await userEvent.click(choice);
 expect(change).toHaveBeenCalledWith({provider:"CLAUDE_CODE",externalId:"live-native-id",label:"对话标题",cwd:"D:\\fictional"});
 expect(screen.getByRole("status")).toHaveTextContent("当前仅观察");
});
it("distinguishes identical names by root and exposes IDs only in identity details",async()=>{
 const api={claudeSessions:vi.fn().mockResolvedValue({sessions:[live("first-native-id","同名","D:\\alpha"),live("second-native-id","同名","D:\\beta")],complete:true,writesEnabled:false})};const change=vi.fn();
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={change}/>);
 await userEvent.click(await screen.findByRole("radio",{name:/同名 · D:\\beta/}));expect(change.mock.calls[0][0].externalId).toBe("second-native-id");
 expect(screen.getByText("second-native-id").closest("details")).not.toHaveAttribute("open");
});
it("missing native title is explicitly unnamed and never pretends the folder is the title",async()=>{
 const api={claudeSessions:vi.fn().mockResolvedValue({sessions:[{...live("native-id",""),title:null}],complete:true,writesEnabled:false})};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 expect(await screen.findByRole("radio",{name:/未命名对话/})).toHaveTextContent("未命名对话");
});
it("connect is explicit, waits for real registration and creates no manual pairing",async()=>{
 const api:ClaudePickerApi={claudeSessions:vi.fn().mockResolvedValue({sessions:[],complete:true,writesEnabled:false}),claudeSetup:vi.fn(),claudeConnect:vi.fn().mockResolvedValue({phase:"WAITING_SESSION",writesEnabled:false})};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 expect(api.claudeConnect).not.toHaveBeenCalled();await userEvent.click(screen.getByRole("button",{name:"连接 Claude Code"}));
 await screen.findByText(/等待 Claude Code 登记对话/);expect(api.claudeConnect).toHaveBeenCalledTimes(1);expect(api.claudeSetup).not.toHaveBeenCalled();expect(screen.queryByText("已连接，可按名称选择对话。")).not.toBeInTheDocument();
});
it("directory refresh preserves row positions during the same visit",async()=>{
 const sessions=[live("a-native","Alpha"),live("b-native","Beta")];const api={claudeSessions:vi.fn().mockResolvedValueOnce({sessions,complete:true,writesEnabled:false}).mockResolvedValue({sessions:[sessions[1],sessions[0]],complete:true,writesEnabled:false})};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 await screen.findByRole("radio",{name:/Alpha/});await userEvent.click(screen.getByRole("button",{name:"刷新目录"}));
 await waitFor(()=>expect(api.claudeSessions).toHaveBeenCalledTimes(2));expect(screen.getAllByRole("radio")[0]).toHaveTextContent("Alpha");
});
it("a genuine catalog arrival replaces waiting without repeating configuration",async()=>{
 const empty={sessions:[],complete:true,writesEnabled:false};const api={claudeSessions:vi.fn().mockResolvedValueOnce(empty).mockResolvedValueOnce(empty).mockResolvedValue({sessions:[live("native-arrived","自然登记")],complete:true,writesEnabled:false}),claudeConnect:vi.fn().mockResolvedValue({phase:"WAITING_SESSION",writesEnabled:false})};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 await userEvent.click(screen.getByRole("button",{name:"连接 Claude Code"}));await screen.findByText(/等待 Claude Code 登记对话/);
 await userEvent.click(screen.getByRole("button",{name:"刷新目录"}));await screen.findByText("已连接，可按名称选择对话。");expect(api.claudeConnect).toHaveBeenCalledTimes(1);expect(screen.queryByText(/等待 Claude Code 登记对话/)).not.toBeInTheDocument();
});
it("connection revoke uses actual connectionId and does not touch another connection",async()=>{
 const api={claudeSessions:vi.fn().mockResolvedValue({sessions:[],complete:true,writesEnabled:false}),claudeConnections:vi.fn().mockResolvedValue([{connectionId:"own-observer",expiresAt:Date.now()+10000,revoked:false,sessionCount:0}]),claudeRevoke:vi.fn().mockResolvedValue(undefined)};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 const button=await screen.findByRole("button",{name:"撤销此连接",hidden:true});await userEvent.click(screen.getByText("手动连接与连接管理"));await userEvent.click(button);
 expect(api.claudeRevoke).toHaveBeenCalledWith("own-observer");
});
it("mobile has metadata selection but no local administrative connector",async()=>{
 const api={claudeSessions:vi.fn().mockResolvedValue({sessions:[],complete:true,writesEnabled:false})};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 expect(screen.queryByRole("button",{name:"连接 Claude Code"})).not.toBeInTheDocument();expect(screen.getByText(/手机端可选择已连接对话/)).toBeInTheDocument();
});
it("arrow navigation uses the displayed exact identities and skips expired sessions",async()=>{
 const sessions=[live("a-native","Alpha"),{...live("b-stale","Beta"),leaseUntil:1},live("c-native","Charlie")];
 const api={claudeSessions:vi.fn().mockResolvedValue({sessions,complete:true,writesEnabled:false})};const change=vi.fn();
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"a-native",label:"Alpha"}} onChange={change}/>);
 const choice=await screen.findByRole("radio",{name:/Alpha/});choice.focus();await userEvent.keyboard("{ArrowDown}");expect(change.mock.calls[0][0].externalId).toBe("c-native");expect(screen.getByRole("radio",{name:/Charlie/})).toHaveFocus();
});
it("directory failure explains Host compatibility and does not create connections",async()=>{
 const api={claudeSessions:vi.fn().mockRejectedValue(Error("unknown IPC")),claudeSetup:vi.fn()};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 await waitFor(()=>expect(screen.getByRole("alert")).toHaveTextContent("Host 已更新"));expect(api.claudeSetup).not.toHaveBeenCalled();
});
