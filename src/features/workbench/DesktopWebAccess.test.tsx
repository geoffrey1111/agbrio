import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {DesktopWebAccess,type WebAccessApi} from "./DesktopWebAccess";
afterEach(cleanup);
it('reinstallation pairs using only the owner-selected old device ID, without deleting it first',async()=>{
 const api={devices:vi.fn().mockResolvedValue([{id:'old-exact',label:'iPhone',createdAt:1,expiresAt:Date.now()/1000+3600},{id:'other-exact',label:'iPhone',createdAt:2,expiresAt:Date.now()/1000+3600}]),issue:vi.fn().mockResolvedValue({code:'123456',expiresAt:Date.now()/1000+300}),revoke:vi.fn(),url:vi.fn().mockResolvedValue('https://qa.invalid/mobile')} satisfies WebAccessApi;
 render(<DesktopWebAccess api={api}/>);fireEvent.click(screen.getByRole('button',{name:'手机/网页登录'}));const select=await screen.findByLabelText('配对方式');fireEvent.change(select,{target:{value:'old-exact'}});fireEvent.click(screen.getByRole('button',{name:'生成配对码'}));await screen.findByLabelText('一次性配对码');expect(api.issue).toHaveBeenCalledExactlyOnceWith('old-exact');expect(api.revoke).not.toHaveBeenCalled();expect(screen.getByText('新登录成功后才替换旧记录。')).toBeVisible();
});
it('hosted onboarding asks for a code alone; self configuration and its draft return on cancel',async()=>{
 const api:WebAccessApi={devices:vi.fn().mockResolvedValue([]),url:vi.fn().mockResolvedValue(null),issue:vi.fn(),revoke:vi.fn(),connection:vi.fn().mockResolvedValue({method:'NONE',origin:null,port:47114,configured:false}),configure:vi.fn()};
 const hostedApi={status:vi.fn().mockResolvedValue({available:true,enabled:false,state:'SELF_HOSTED',origin:null,expiresAt:null}),redeem:vi.fn()};
 render(<DesktopWebAccess api={api} hostedApi={hostedApi}/>);fireEvent.click(screen.getByRole('button',{name:'手机/网页登录'}));fireEvent.click(await screen.findByRole('button',{name:'兑换码'}));expect(screen.queryByLabelText('HTTPS 网址')).toBeNull();expect(screen.queryByLabelText('连接方式')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'取消'}));fireEvent.click(screen.getByRole('button',{name:'自行配置'}));fireEvent.change(screen.getByLabelText('HTTPS 网址'),{target:{value:'https://own.invalid'}});fireEvent.click(screen.getByRole('button',{name:'兑换码'}));expect(screen.queryByLabelText('HTTPS 网址')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'取消'}));expect(screen.getByLabelText('HTTPS 网址')).toHaveValue('https://own.invalid');expect(hostedApi.redeem).not.toHaveBeenCalled();
});
it('lets a fresh user save a verified provider entry without generating a pairing code',async()=>{
 const configure=vi.fn().mockResolvedValue({method:'TAILSCALE_FUNNEL',origin:'https://pc.tail1.ts.net',port:47114,configured:true});
 const api:WebAccessApi={devices:vi.fn().mockResolvedValue([]),url:vi.fn().mockResolvedValue(null),issue:vi.fn(),revoke:vi.fn(),connection:vi.fn().mockResolvedValue({method:'NONE',origin:null,port:47114,configured:false}),configure};
 render(<DesktopWebAccess api={api}/>);fireEvent.click(screen.getByRole('button',{name:'手机/网页登录'}));
 await screen.findByLabelText('HTTPS 网址');expect(screen.getByRole('button',{name:'验证并保存'})).toBeDisabled();
 expect(screen.queryByRole('button',{name:'生成配对码'})).not.toBeInTheDocument();
 fireEvent.change(screen.getByLabelText('HTTPS 网址'),{target:{value:'https://pc.tail1.ts.net'}});fireEvent.click(screen.getByRole('button',{name:'验证并保存'}));
 await waitFor(()=>expect(configure).toHaveBeenCalledExactlyOnceWith({method:'TAILSCALE_FUNNEL',origin:'https://pc.tail1.ts.net'}));await screen.findByRole('link',{name:'https://pc.tail1.ts.net/mobile'});expect(api.issue).not.toHaveBeenCalled();
});
it('keeps the old URL after a failed HTTPS verification',async()=>{
 const api:WebAccessApi={devices:vi.fn().mockResolvedValue([]),url:vi.fn().mockResolvedValue('https://old.example/mobile'),issue:vi.fn(),revoke:vi.fn(),connection:vi.fn().mockResolvedValue({method:'CLOUDFLARE',origin:'https://old.example',port:47114,configured:true}),configure:vi.fn().mockRejectedValue(Error('MOBILE_PROBE_FAILED'))};
 render(<DesktopWebAccess api={api}/>);fireEvent.click(screen.getByRole('button',{name:'手机/网页登录'}));fireEvent.click(await screen.findByRole('button',{name:'更换连接'}));fireEvent.change(screen.getByLabelText('HTTPS 网址'),{target:{value:'https://new.example'}});fireEvent.click(screen.getByRole('button',{name:'验证并保存'}));await screen.findByText('未能验证这个入口，原连接已保留。请检查转发和 HTTPS 配置。');expect(screen.getByRole('link',{name:'https://old.example/mobile'})).toBeVisible();expect(api.issue).not.toHaveBeenCalled();
});
it("issues a code only on explicit action and revokes the exact displayed device",async()=>{
 const api={devices:vi.fn().mockResolvedValue([{id:"device-exact",label:"我的手机",createdAt:1,expiresAt:Date.now()/1000+3600}]),issue:vi.fn().mockResolvedValue({code:"123456",expiresAt:Date.now()/1000+300}),revoke:vi.fn().mockResolvedValue({}),url:vi.fn().mockResolvedValue("https://router.example/mobile")} satisfies WebAccessApi;
 render(<DesktopWebAccess api={api}/>);fireEvent.click(screen.getByRole("button",{name:"手机/网页登录"}));await screen.findByRole("button",{name:"生成配对码"});expect(api.issue).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"生成配对码"}));expect(await screen.findByLabelText("一次性配对码")).toHaveTextContent("123456");
 await waitFor(()=>expect(screen.getByRole("button",{name:"退出此设备：我的手机"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"退出此设备：我的手机"}));expect(api.revoke).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"退出此设备"}));await waitFor(()=>expect(api.revoke).toHaveBeenCalledWith("device-exact"));
});
