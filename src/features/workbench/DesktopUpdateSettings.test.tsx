import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {DesktopUpdateSettings,type UpdateApi,type UpdateView} from "./DesktopUpdateSettings";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
const idle:UpdateView={state:"IDLE",currentVersion:"0.1.0",version:null,notes:null,releaseUrl:null,downloaded:0,total:null,error:null};
const next={...idle,state:"AVAILABLE",version:"0.1.1",notes:"Improve Bridge activity."};
function api():UpdateApi{return {status:vi.fn().mockResolvedValue(idle),check:vi.fn().mockResolvedValue(next),download:vi.fn().mockResolvedValue({...next,state:"READY"}),install:vi.fn().mockResolvedValue(undefined),openRelease:vi.fn().mockResolvedValue(undefined)};}
it("checks only on an owner click and installs only after a verified download and a separate install click",async()=>{
 const a=api();render(<DesktopUpdateSettings updateApi={a}/>);await screen.findByText(/当前版本/);expect(a.check).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"检查更新"}));fireEvent.click(await screen.findByRole("button",{name:"下载更新"}));const install=await screen.findByRole("button",{name:"安装并重启 Agbrio"});expect(a.install).not.toHaveBeenCalled();fireEvent.click(install);await waitFor(()=>expect(a.install).toHaveBeenCalledExactlyOnceWith("0.1.1"));
});
it("signature or network failures unlock retry without installation or false success",async()=>{
 const a=api();vi.mocked(a.check).mockRejectedValue(Error("UPDATE_NETWORK"));render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"检查更新"}));expect(await screen.findByRole("alert")).toHaveTextContent("暂时无法检查更新");await waitFor(()=>expect(screen.getByRole("button",{name:"检查更新"})).toBeEnabled());expect(a.install).not.toHaveBeenCalled();
});
it("shows unsigned-release absence honestly in English",async()=>{
 setLanguagePreference("en");const a=api();vi.mocked(a.check).mockResolvedValue({...idle,state:"NO_SIGNED_RELEASE"});render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"Check for updates"}));expect(await screen.findByText("No in-app update has been published yet")).toBeVisible();expect(screen.queryByRole("button",{name:"Install and restart Agbrio"})).toBeNull();expect(a.download).not.toHaveBeenCalled();
});
it("prevents repeated download clicks while verification is pending",async()=>{
 const a=api();let finish!:(value:UpdateView)=>void;vi.mocked(a.download).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"检查更新"}));const download=await screen.findByRole("button",{name:"下载更新"});fireEvent.click(download);fireEvent.click(download);expect(a.download).toHaveBeenCalledExactlyOnceWith("0.1.1");expect(a.install).not.toHaveBeenCalled();finish({...next,state:"READY"});expect(await screen.findByRole("button",{name:"安装并重启 Agbrio"})).toBeEnabled();
});
it("retains the verified download after an install backup failure",async()=>{
 const a=api();vi.mocked(a.status).mockResolvedValue({...next,state:"READY"});vi.mocked(a.install).mockRejectedValue(Error("UPDATE_BACKUP_FAILED"));render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"安装并重启 Agbrio"}));await screen.findByRole("alert");expect(screen.queryByRole("dialog")).toBeNull();expect(a.download).not.toHaveBeenCalled();await waitFor(()=>expect(screen.getByRole("button",{name:"安装并重启 Agbrio"})).toBeEnabled());
});

it("requires a second confirmation for the same verified candidate",async()=>{
 const a=api();vi.mocked(a.status).mockResolvedValue({...next,state:"READY"});vi.mocked(a.install).mockRejectedValueOnce(Error("UPDATE_TASK_RUNNING")).mockResolvedValue(undefined);
 render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"安装并重启 Agbrio"}));await screen.findByRole("dialog");expect(a.install).toHaveBeenCalledTimes(1);
 fireEvent.click(screen.getByRole("button",{name:"取消"}));expect(screen.queryByRole("dialog")).toBeNull();expect(a.install).toHaveBeenCalledTimes(1);
 vi.mocked(a.install).mockRejectedValueOnce(Error("UPDATE_TASK_RUNNING"));fireEvent.click(screen.getByRole("button",{name:"安装并重启 Agbrio"}));await screen.findByRole("dialog");fireEvent.click(screen.getByRole("button",{name:"确认强制更新"}));await waitFor(()=>expect(a.install).toHaveBeenLastCalledWith("0.1.1",true));expect(a.download).not.toHaveBeenCalled();
});

it("rejects a changed candidate while a force confirmation is open",async()=>{const a=api();let emit!:(value:UpdateView)=>void;a.subscribe=async cb=>{emit=cb;return()=>{}};vi.mocked(a.status).mockResolvedValue({...next,state:"READY"});vi.mocked(a.install).mockRejectedValue(Error("UPDATE_TASK_RUNNING"));render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"安装并重启 Agbrio"}));await screen.findByRole("dialog");const {act}=await import('@testing-library/react');act(()=>emit({...next,state:"READY",version:"0.1.2"}));expect(screen.getByRole("button",{name:"确认强制更新"})).toBeDisabled();expect(a.install).toHaveBeenCalledTimes(1);});
it("renders background download events and only changes silent install after user opt-in",async()=>{
 const a=api();let emit!:(value:UpdateView)=>void;
 a.subscribe=async cb=>{emit=cb;return()=>{};};
 a.setAutoInstall=vi.fn().mockResolvedValue({...next,state:"READY",autoInstall:true});
 render(<DesktopUpdateSettings updateApi={a}/>);
 const option=await screen.findByRole("checkbox",{name:"空闲时自动安装更新"});
 const {act}=await import("@testing-library/react");
 await act(async()=>emit({...next,state:"DOWNLOADING",downloaded:3,total:6}));
 expect(screen.getByRole("progressbar")).toHaveValue(3);
 expect(option).toBeDisabled();
 expect(a.download).not.toHaveBeenCalled();
 await act(async()=>emit({...next,state:"READY"}));
 expect(option).not.toBeChecked();
 fireEvent.click(option);
 expect(await screen.findByText("已下载，将在窗口关闭且任务空闲时安装")).toBeVisible();
 expect(a.setAutoInstall).toHaveBeenCalledExactlyOnceWith(true);
 expect(a.install).not.toHaveBeenCalled();
});
