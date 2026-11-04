import { cleanup,fireEvent,render,screen,waitFor } from "@testing-library/react";
import { afterEach,describe,expect,it,vi } from "vitest";
import { NotificationDeliveryPanel,type DeliveryApi,type DeliveryView } from "./NotificationDeliveryPanel";
vi.mock("../../mobile/push",()=>({pushSetupState:vi.fn().mockResolvedValue("SUPPORTED_NOT_SUBSCRIBED"),enableWebPush:vi.fn().mockResolvedValue(undefined),disableWebPush:vi.fn().mockResolvedValue(undefined)}));
import { enableWebPush } from "../../mobile/push";
afterEach(cleanup);
function fixture(desktop=true){const view:DeliveryView={account:"geoffreyzjx826@gmail.com",recipient:"geoffreyzjx826@gmail.com",hasSecret:false,windowsStatus:"ENABLED",channels:[{channel:"WINDOWS",enabled:false},{channel:"EMAIL",enabled:false}],history:[]};return {desktop,settings:vi.fn().mockResolvedValue(view),command:vi.fn().mockResolvedValue("SAVED")} satisfies DeliveryApi;}
describe("notification delivery setup",()=>{
 it("lets first-use Windows send a test before enabling ongoing reminders",async()=>{const api=fixture();const initial=await api.settings();api.settings.mockReset().mockResolvedValueOnce({...initial,windowsStatus:"NOT_INITIALIZED"}).mockResolvedValue(initial);render(<NotificationDeliveryPanel api={api}/>);expect(await screen.findByText("首次使用，请先发送测试通知，然后开启提醒。",{exact:false})).toBeInTheDocument();expect(screen.getByRole("button",{name:"开启 Windows 提醒"})).toBeDisabled();fireEvent.click(screen.getByRole("button",{name:"测试 Windows 通知"}));await waitFor(()=>expect(screen.getByRole("button",{name:"开启 Windows 提醒"})).toBeEnabled());expect(api.command).toHaveBeenCalledExactlyOnceWith({action:"TEST",channel:"WINDOWS"});});
 it("requires a user gesture for browser permission and hides desktop credential setup on web",async()=>{vi.mocked(enableWebPush).mockClear();const api=fixture(false);render(<NotificationDeliveryPanel api={api}/>);await screen.findByText("尚未订阅");expect(enableWebPush).not.toHaveBeenCalled();expect(screen.queryByLabelText("应用专用密码")).not.toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"开启此浏览器推送"}));await waitFor(()=>expect(api.command).toHaveBeenCalledWith({action:"CHANNEL",channel:"WEB",enabled:true}));});
 it("saves the corrected email and clears the password input; no send happens before test",async()=>{const api=fixture();render(<NotificationDeliveryPanel api={api}/>);const input=await screen.findByLabelText("应用专用密码");expect(screen.getByLabelText("收件邮箱")).toHaveValue("geoffreyzjx826@gmail.com");expect(screen.getByRole("button",{name:"发送测试邮件"})).toBeDisabled();fireEvent.change(input,{target:{value:"abcdefghijklmnop"}});fireEvent.click(screen.getByRole("button",{name:"保存邮件设置"}));await waitFor(()=>expect(input).toHaveValue(""));expect(api.command).toHaveBeenCalledExactlyOnceWith({action:"EMAIL",account:"geoffreyzjx826@gmail.com",recipient:"geoffreyzjx826@gmail.com",password:"abcdefghijklmnop"});});
});

it('saves the selected notification categories without disabling channels',async()=>{const api=fixture(false);const initial=await api.settings();api.settings.mockResolvedValue({...initial,notificationTypes:{completed:true,failed:true,actionRequired:true}});render(<NotificationDeliveryPanel api={api}/>);fireEvent.click(await screen.findByRole('checkbox',{name:'任务完成'}));await waitFor(()=>expect(api.command).toHaveBeenCalledWith({action:'NOTIFICATION_TYPES',types:{completed:false,failed:true,actionRequired:true}}));});

it('asks macOS notification permission only after an explicit owner action',async()=>{
 const view:DeliveryView={desktopPlatform:'MACOS',account:'fixture@example.invalid',recipient:'fixture@example.invalid',hasSecret:false,windowsStatus:'NOT_INITIALIZED',channels:[{channel:'WINDOWS',enabled:false}],history:[]};
 const api:DeliveryApi={desktop:true,settings:vi.fn().mockResolvedValue(view),command:vi.fn().mockResolvedValue('SAVED')};render(<NotificationDeliveryPanel api={api}/>);
 await screen.findByRole('heading',{name:'macOS 桌面通知'});expect(api.command).not.toHaveBeenCalled();expect(screen.getByLabelText('应用专用密码')).toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'开启 macOS 提醒'}));await waitFor(()=>expect(api.command).toHaveBeenCalledWith({action:'CHANNEL',channel:'WINDOWS',enabled:true}));
 expect(vi.mocked(api.command).mock.calls[0]).toEqual([{action:'AUTHORIZE_DESKTOP'}]);
});

it('does not enable macOS delivery after permission is denied or remains uncertain',async()=>{
 const view:DeliveryView={desktopPlatform:'MACOS',account:'',recipient:'',hasSecret:false,windowsStatus:'NOT_INITIALIZED',channels:[{channel:'WINDOWS',enabled:false}],history:[]};
 const api:DeliveryApi={desktop:true,settings:vi.fn().mockResolvedValue(view),command:vi.fn().mockRejectedValue('MACOS_NOTIFICATION_PERMISSION_PENDING')};render(<NotificationDeliveryPanel api={api}/>);
 fireEvent.click(await screen.findByRole('button',{name:'开启 macOS 提醒'}));await screen.findByRole('alert');expect(api.command).toHaveBeenCalledExactlyOnceWith({action:'AUTHORIZE_DESKTOP'});
});


it('authorizes a locked Mac email credential only on click without sending or enabling',async()=>{
 const view:DeliveryView={desktopPlatform:'MACOS',credentialStatus:'NEEDS_OWNER_ACTION',account:'fixture@example.invalid',recipient:'fixture@example.invalid',hasSecret:false,windowsStatus:'ENABLED',channels:[{channel:'EMAIL',enabled:false}],history:[]};
 const api:DeliveryApi={desktop:true,settings:vi.fn().mockResolvedValue(view),command:vi.fn().mockResolvedValue('AUTHORIZED')};render(<NotificationDeliveryPanel api={api}/>);
 const authorize=await screen.findByRole('button',{name:'授权读取钥匙串'});expect(api.command).not.toHaveBeenCalled();expect(screen.getByRole('button',{name:'发送测试邮件'})).toBeDisabled();
 fireEvent.change(screen.getByLabelText('发信 Gmail 账号'),{target:{value:'unsaved@example.invalid'}});expect(authorize).toBeDisabled();fireEvent.change(screen.getByLabelText('发信 Gmail 账号'),{target:{value:view.account}});expect(authorize).not.toBeDisabled();fireEvent.click(authorize);await screen.findByText('钥匙串读取已获准。没有发送邮件或开启提醒。');expect(api.command).toHaveBeenCalledExactlyOnceWith({action:'AUTHORIZE_EMAIL_KEYCHAIN'});
});

it('keeps the Mac authorization failure visible without retrying or sending',async()=>{
 const view:DeliveryView={desktopPlatform:'MACOS',credentialStatus:'NEEDS_OWNER_ACTION',account:'fixture@example.invalid',recipient:'fixture@example.invalid',hasSecret:false,windowsStatus:'ENABLED',channels:[],history:[]};
 const api:DeliveryApi={desktop:true,settings:vi.fn().mockResolvedValue(view),command:vi.fn().mockRejectedValue('KEYCHAIN_OWNER_ACTION_REQUIRED')};render(<NotificationDeliveryPanel api={api}/>);
 fireEvent.click(await screen.findByRole('button',{name:'授权读取钥匙串'}));expect(await screen.findByRole('alert')).toHaveTextContent('macOS 钥匙串需要本人授权或解锁');expect(api.command).toHaveBeenCalledExactlyOnceWith({action:'AUTHORIZE_EMAIL_KEYCHAIN'});
});
