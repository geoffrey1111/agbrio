import {afterEach,expect,it} from "vitest";
import {cleanup,render,screen} from "@testing-library/react";
import {CodexConnection,type CodexConnectionStatus} from "./CodexConnection";
afterEach(cleanup);
const show=(status:CodexConnectionStatus)=>render(<CodexConnection status={async()=>status}/>);
it("reports the connection with nothing to prepare, launch or switch",async()=>{
 show({enabled:true,ready:true,desktopConnected:true,state:"LIGHT_CONNECTED"});
 expect(await screen.findByRole("heading",{name:"轻量连接已接入 Codex"})).toBeVisible();
 expect(screen.getByText(/不需要退出或专用入口/)).toBeVisible();
 expect(screen.getByText(/轻量连接不支持：管理 Goal/)).toBeVisible();
 expect(screen.queryByRole("button")).toBeNull();
});
it("waits for a normally opened Codex",async()=>{
 show({enabled:true,ready:true,desktopConnected:false,state:"LIGHT_WAITING"});
 expect(await screen.findByRole("heading",{name:"轻量连接已开启"})).toBeVisible();
 expect(screen.getByText("请正常打开 Codex，Agbrio 会自动连接。")).toBeVisible();
});
it("says an old shared backend still in use is left alone and costs the connection for now",async()=>{
 show({enabled:true,ready:false,desktopConnected:false,state:"SHARED_RETIRING"});
 expect(await screen.findByRole("status")).toHaveTextContent("不会中断任务");
 expect(screen.getByRole("status")).toHaveTextContent("暂时连不上 Codex");
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
