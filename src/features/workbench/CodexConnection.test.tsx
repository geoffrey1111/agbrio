import {afterEach,expect,it,vi} from "vitest";
import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {CodexConnection,type CodexConnectionStatus} from "./CodexConnection";
afterEach(cleanup);
const show=(status:CodexConnectionStatus)=>render(<CodexConnection status={async()=>status}/>);
it("reports the connection with nothing to prepare, launch or switch",async()=>{
 show({enabled:true,ready:true,desktopConnected:true,state:"LIGHT_CONNECTED"});
 expect(await screen.findByRole("heading",{name:"轻量连接已接入 Codex"})).toBeVisible();
 expect(screen.getByText(/不需要退出或专用入口/)).toBeVisible();
 expect(screen.queryByText(/管理 Goal|管理员/)).toBeNull();
 expect(screen.queryByRole("button")).toBeNull();
});
it("waits for a normally opened Codex",async()=>{
 show({enabled:true,ready:true,desktopConnected:false,state:"LIGHT_WAITING"});
 expect(await screen.findByRole("heading",{name:"轻量连接已开启"})).toBeVisible();
 expect(screen.getByText("请正常打开 Codex，Agbrio 会自动连接。")).toBeVisible();
});
it("offers to stop an old shared backend, only after a second confirmation",async()=>{
 const retire=vi.fn(async()=>{});
 render(<CodexConnection status={async()=>({enabled:true,ready:false,desktopConnected:false,state:"SHARED_RETIRING",sharedActiveTurns:1,sharedDesktopConnected:true})} retire={retire}/>);
 expect(await screen.findByRole("status")).toHaveTextContent("暂时连不上 Codex");
 fireEvent.click(screen.getByRole("button",{name:"关闭共享后台"}));
 expect(retire).not.toHaveBeenCalled();
 expect(await screen.findByText("正在运行的 1 个任务会中断，不会自动重发。")).toBeVisible();
 fireEvent.click(screen.getByRole("dialog").querySelector(".r2-action")!);
 await waitFor(()=>expect(retire).toHaveBeenCalledOnce());
});
it("reports the stop while it runs",async()=>{
 show({enabled:true,ready:false,desktopConnected:false,state:"SHARED_STOPPING"});
 expect(await screen.findByRole("heading",{name:"正在关闭旧的共享后台"})).toBeVisible();
 expect(screen.queryByRole("button")).toBeNull();
});
it("distinguishes a declined Windows consent and shows the code of any other refusal",async()=>{
 const view=show({enabled:true,ready:false,desktopConnected:false,state:"SHARED_RETIREMENT_BLOCKED",detail:"SHARED_UAC_CANCELLED"});
 expect(await screen.findByRole("alert")).toHaveTextContent("已取消 Windows 授权");
 view.unmount();
 show({enabled:true,ready:false,desktopConnected:false,state:"SHARED_RETIREMENT_BLOCKED",detail:"SHARED_CONNECTION_UNCERTAIN"});
 expect(await screen.findByRole("alert")).toHaveTextContent("没有结束任何进程");
 expect(screen.getByText("SHARED_CONNECTION_UNCERTAIN")).toBeVisible();
 expect(screen.getByRole("alert")).toHaveTextContent("重新打开 Agbrio 后会再试一次");
});
it("offers nothing to set up on a platform without the light connection",async()=>{
 show({enabled:false,ready:false,desktopConnected:false,state:"UNSUPPORTED"});
 expect(await screen.findByText(/独立的 Codex 后台/)).toBeVisible();
 expect(screen.queryByRole("button")).toBeNull();
});
it("reports an unreadable status instead of a state it does not know",async()=>{
 render(<CodexConnection status={()=>Promise.reject(Error("read failed"))}/>);
 expect(await screen.findByRole("alert")).toHaveTextContent("连接状态暂时无法读取");
});
