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
it("retains a verified package when installation is blocked by an active task",async()=>{
 const a=api();vi.mocked(a.status).mockResolvedValue({...next,state:"READY"});vi.mocked(a.install).mockRejectedValue(Error("UPDATE_TASK_RUNNING"));render(<DesktopUpdateSettings updateApi={a}/>);fireEvent.click(await screen.findByRole("button",{name:"安装并重启 Agbrio"}));expect(await screen.findByRole("alert")).toHaveTextContent("任务仍在执行");await waitFor(()=>expect(screen.getByRole("button",{name:"安装并重启 Agbrio"})).toBeEnabled());expect(a.download).not.toHaveBeenCalled();
});
