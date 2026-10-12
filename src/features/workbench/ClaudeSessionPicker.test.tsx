import {render,screen,waitFor,cleanup} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {afterEach,expect,it,vi} from "vitest";
import {ClaudeSessionPicker,type ClaudePickerApi} from "./ClaudeSessionPicker";
afterEach(cleanup);
it("selects exact native identity; stale rows cannot be chosen and setup is explicit",async()=>{
 const sessions=[{sessionId:"live-native-id",cwd:"D:\\fictional",version:"2.1.286",state:"COMPLETE",checkedAt:Date.now(),leaseUntil:Date.now()+45000},{sessionId:"stale-native-id",cwd:"D:\\old",version:"2.1.286",state:"COMPLETE",checkedAt:1,leaseUntil:1}];
 const api:ClaudePickerApi={claudeSessions:vi.fn().mockResolvedValue({sessions,complete:true,writesEnabled:false}),claudeSetup:vi.fn()};const change=vi.fn();
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={change}/>);
 await screen.findByRole("option",{name:/D:\\fictional/});
 expect(api.claudeSetup).not.toHaveBeenCalled();
 expect(screen.getByRole("option",{name:/D:\\old/})).toBeDisabled();
 await userEvent.selectOptions(screen.getByRole("combobox"),"live-native-id");
 expect(change).toHaveBeenCalledWith({provider:"CLAUDE_CODE",externalId:"live-native-id",label:"Claude Code · fictional",cwd:"D:\\fictional"});
 expect(screen.getByRole("status")).toHaveTextContent("当前仅观察");
});
it("directory failure explains Host compatibility and does not create connections",async()=>{
 const api={claudeSessions:vi.fn().mockRejectedValue(Error("unknown IPC")),claudeSetup:vi.fn()};
 render(<ClaudeSessionPicker api={api} value={{provider:"CLAUDE_CODE",externalId:"",label:""}} onChange={()=>{}}/>);
 await waitFor(()=>expect(screen.getByRole("alert")).toHaveTextContent("Host 已更新"));
 expect(api.claudeSetup).not.toHaveBeenCalled();
});
