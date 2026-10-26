import {cleanup,fireEvent,render,screen,waitFor,act} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {SharedCodexConnection,type SharedCodexStatus} from "./SharedCodexConnection";
afterEach(()=>{cleanup();vi.useRealTimers();});
const independent:SharedCodexStatus={enabled:false,ready:false,desktopConnected:false,state:"INDEPENDENT"};
const ready:SharedCodexStatus={enabled:false,ready:true,desktopConnected:false,state:"SHARED_PREPARED"};
it('distinguishes the next-start choice from the running backend token',async()=>{
 render(<SharedCodexConnection api={{status:async()=>({...ready,launchAsAdministrator:true,currentAdministrator:false}),setup:vi.fn(),setAdministrator:vi.fn()}}/>);
 expect(await screen.findByText('当前后台：普通权限')).toBeVisible();expect(screen.getByRole('checkbox',{name:'以管理员身份启动共享 Codex'})).toBeChecked();
});
it('defaults administrator launch on and saves an explicit opt-out without starting anything',async()=>{
 const launch=vi.fn(),setup=vi.fn(),setAdministrator=vi.fn().mockResolvedValue({...ready,launchAsAdministrator:false});
 render(<SharedCodexConnection api={{status:async()=>ready,launch,setup,setAdministrator}}/>);
 const checkbox=await screen.findByRole('checkbox',{name:'以管理员身份启动共享 Codex'});
 await waitFor(()=>expect(checkbox).toBeEnabled());expect(checkbox).toBeChecked();fireEvent.click(checkbox);
 await waitFor(()=>expect(checkbox).not.toBeChecked());expect(setAdministrator).toHaveBeenCalledWith(false);expect(launch).not.toHaveBeenCalled();expect(setup).not.toHaveBeenCalled();
});
it('restores a saved standard-mode choice and retains it when a preference write fails',async()=>{
 const setAdministrator=vi.fn().mockRejectedValue('SHARED_LAUNCH_PREFERENCES_UNAVAILABLE');
 render(<SharedCodexConnection api={{status:async()=>({...ready,launchAsAdministrator:false}),setup:vi.fn(),setAdministrator}}/>);
 const checkbox=await screen.findByRole('checkbox',{name:'以管理员身份启动共享 Codex'});await waitFor(()=>expect(checkbox).toBeEnabled());expect(checkbox).not.toBeChecked();fireEvent.click(checkbox);
 expect(await screen.findByRole('alert')).toHaveTextContent('启动权限设置未保存');expect(checkbox).not.toBeChecked();
});
it('incomplete native components report that shared mode did not start',async()=>{
 const api={status:vi.fn().mockResolvedValue(independent),setup:vi.fn().mockRejectedValue('SHARED_NATIVE_RUNTIME_INCOMPLETE'),launch:vi.fn()};
 render(<SharedCodexConnection api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'准备共享连接'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('共享连接未启动');expect(api.launch).not.toHaveBeenCalled();
});
it('uncertain recovery reports no process stop instead of a perpetual recovery claim',async()=>{
 const api={status:vi.fn().mockResolvedValue({...independent,enabled:true,state:'SHARED_RECONNECTING'}),setup:vi.fn().mockRejectedValue('SHARED_CONNECTION_UNCERTAIN')};
 render(<SharedCodexConnection api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'恢复共享连接'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('未结束进程');expect(screen.queryByText('正在恢复共享连接')).not.toBeInTheDocument();
});
it('polls real launch progress during the pending operation',async()=>{
 vi.useFakeTimers();let finish!:(s:SharedCodexStatus)=>void;
 const api={status:vi.fn().mockResolvedValueOnce(ready).mockResolvedValue({...ready,launchProgress:{step:'CONNECTING',state:'RUNNING',updatedAt:Date.now()}}),setup:vi.fn(),launch:vi.fn(()=>new Promise<SharedCodexStatus>(r=>{finish=r;}))};
 render(<SharedCodexConnection api={api}/>);await act(async()=>{});fireEvent.click(screen.getByRole('button',{name:'打开共享 Codex'}));await act(async()=>vi.advanceTimersByTime(1000));
 expect(screen.getByRole('status')).toHaveTextContent('连接共享后端');expect(screen.getByRole('button',{name:'正在处理…'})).toBeDisabled();await act(async()=>finish({...ready,enabled:true,desktopConnected:true,state:'SHARED_CONNECTED'}));
 expect(screen.getByText('Desktop 已接入共享连接')).toBeVisible();
});
it('shows preparation immediately and excludes a previous failed attempt from the new progress',async()=>{
 let finish!:(s:SharedCodexStatus)=>void;
 const api={status:vi.fn().mockResolvedValue({...independent,launchProgress:{step:'FAILED',state:'FAILED',updatedAt:1}}),setup:vi.fn(()=>new Promise<SharedCodexStatus>(r=>{finish=r;}))};
 render(<SharedCodexConnection api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'准备共享连接'}));
 expect(screen.getByRole('status')).toHaveTextContent('检查并准备共享连接');expect(screen.getByRole('status')).toHaveTextContent('1/4');
 expect(screen.queryByText('启动未完成')).not.toBeInTheDocument();expect(screen.getByRole('progressbar',{name:'共享启动进度'})).toHaveAttribute('value','0');
 await act(async()=>finish(ready));expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
});
it('distinguishes failed window verification from a requested change of backend permissions',async()=>{
 render(<SharedCodexConnection api={{status:async()=>ready,setup:vi.fn(),launch:vi.fn().mockRejectedValue('SHARED_PRIVILEGE_TOKEN_MISMATCH')}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'打开共享 Codex'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('共享窗口启动校验失败');expect(screen.getByRole('alert')).not.toHaveTextContent('当前设置不一致');
});
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

it("a disconnected backend offers explicit recovery without starting automatically",async()=>{
 const api={status:vi.fn().mockResolvedValue({...ready,enabled:true,ready:false,state:"SHARED_RECONNECTING"}),setup:vi.fn().mockResolvedValue(ready)};render(<SharedCodexConnection api={api}/>);await screen.findByText("共享连接已断开");expect(api.setup).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"恢复共享连接"}));await screen.findByText("共享连接已准备好");expect(api.setup).toHaveBeenCalledTimes(1);
});

it("prepared mode needs explicit launch and offers disable without reinstalling certificates",async()=>{
 const prepared={...independent,ready:true,state:"SHARED_PREPARED"};const active={...prepared,enabled:true,desktopConnected:true,state:"SHARED_CONNECTED"};
 const api={status:vi.fn().mockResolvedValue(independent),setup:vi.fn().mockResolvedValue(prepared),launch:vi.fn().mockResolvedValue(active),disable:vi.fn().mockResolvedValue(independent)};
 render(<SharedCodexConnection api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"准备共享连接"}));
 await screen.findByRole("button",{name:"打开共享 Codex"});expect(api.launch).not.toHaveBeenCalled();expect(screen.getByText(/默认启动保持不变/)).toBeInTheDocument();
 fireEvent.click(screen.getByRole("button",{name:"打开共享 Codex"}));await screen.findByText("Desktop 已接入共享连接");
 fireEvent.click(screen.getByRole("button",{name:"停用共享连接"}));await screen.findByRole("button",{name:"准备共享连接"});expect(api.disable).toHaveBeenCalledTimes(1);
});
