import {cleanup,fireEvent,render,screen,waitFor,act} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {SharedCodexConnection,type SharedCodexStatus} from "./SharedCodexConnection";
afterEach(()=>{cleanup();vi.useRealTimers();});
const independent:SharedCodexStatus={enabled:false,ready:false,desktopConnected:false,state:"INDEPENDENT"};
const ready:SharedCodexStatus={enabled:false,ready:true,desktopConnected:false,state:"SHARED_PREPARED"};
it("requires explicit preparation and a separate launch before reporting shared connection",async()=>{
 const active={...ready,enabled:true,desktopConnected:true,state:"SHARED_CONNECTED"};const api={status:vi.fn().mockResolvedValue(independent),setup:vi.fn().mockResolvedValue(ready),launch:vi.fn().mockResolvedValue(active)};
 render(<SharedCodexConnection api={api}/>);await waitFor(()=>expect(screen.getByRole("button",{name:"准备共享连接"})).toBeEnabled());expect(api.setup).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"准备共享连接"}));await screen.findByText("共享连接已准备好");expect(screen.queryByText("Desktop 已接入共享连接")).not.toBeInTheDocument();expect(api.launch).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"打开共享 Codex"}));await screen.findByText("Desktop 已接入共享连接");expect(api.launch).toHaveBeenCalledTimes(1);expect(api.setup).toHaveBeenCalledTimes(1);
});
it("late polling cannot overwrite an explicit setup result",async()=>{
 vi.useFakeTimers();let finishPoll!:(s:SharedCodexStatus)=>void;
 const api={status:vi.fn().mockResolvedValueOnce(independent).mockImplementationOnce(()=>new Promise<SharedCodexStatus>(r=>{finishPoll=r;})),setup:vi.fn().mockResolvedValue(ready)};
 render(<SharedCodexConnection api={api}/>);await act(async()=>{});await act(async()=>vi.advanceTimersByTime(10000));
 fireEvent.click(screen.getByRole("button",{name:"准备共享连接"}));await act(async()=>{});await act(async()=>finishPoll(independent));expect(screen.getByText("共享连接已准备好")).toBeInTheDocument();
});
it("a failed setup remains retryable and does not imply connection",async()=>{
 const api={status:vi.fn().mockResolvedValue(independent),setup:vi.fn().mockRejectedValue("SHARED_ROUTER_BUSY")};render(<SharedCodexConnection api={api}/>);await waitFor(()=>expect(screen.getByRole("button",{name:"准备共享连接"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"准备共享连接"}));expect(await screen.findByRole("alert")).toHaveTextContent("对话正在执行");expect(screen.getByRole("button",{name:"准备共享连接"})).toBeEnabled();
});
it("preserves the connected state while deferring a runtime update",async()=>{
 const api={status:vi.fn().mockResolvedValue({...ready,enabled:true,desktopConnected:true,updatePending:true}),setup:vi.fn()};render(<SharedCodexConnection api={api}/>);await screen.findByText("Desktop 已接入共享连接");expect(screen.getByRole("status")).toHaveTextContent("停用后重新准备");expect(api.setup).not.toHaveBeenCalled();
});

it("checking a disconnected shared backend never repeats certificate setup",async()=>{
 const api={status:vi.fn().mockResolvedValue({...ready,enabled:true,ready:false,state:"SHARED_RECONNECTING"}),setup:vi.fn()};render(<SharedCodexConnection api={api}/>);await screen.findByText("正在恢复共享连接");fireEvent.click(screen.getByRole("button",{name:"检查连接"}));await waitFor(()=>expect(api.status).toHaveBeenCalledTimes(2));expect(api.setup).not.toHaveBeenCalled();
});

it("prepared mode needs explicit launch and offers disable without reinstalling certificates",async()=>{
 const prepared={...independent,ready:true,state:"SHARED_PREPARED"};const active={...prepared,enabled:true,desktopConnected:true,state:"SHARED_CONNECTED"};
 const api={status:vi.fn().mockResolvedValue(independent),setup:vi.fn().mockResolvedValue(prepared),launch:vi.fn().mockResolvedValue(active),disable:vi.fn().mockResolvedValue(independent)};
 render(<SharedCodexConnection api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"准备共享连接"}));
 await screen.findByRole("button",{name:"打开共享 Codex"});expect(api.launch).not.toHaveBeenCalled();expect(screen.getByText(/默认启动保持不变/)).toBeInTheDocument();
 fireEvent.click(screen.getByRole("button",{name:"打开共享 Codex"}));await screen.findByText("Desktop 已接入共享连接");
 fireEvent.click(screen.getByRole("button",{name:"停用共享连接"}));await screen.findByRole("button",{name:"准备共享连接"});expect(api.disable).toHaveBeenCalledTimes(1);
});
