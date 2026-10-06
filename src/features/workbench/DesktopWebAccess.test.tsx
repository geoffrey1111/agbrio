import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {DesktopWebAccess,type WebAccessApi} from "./DesktopWebAccess";
afterEach(cleanup);
it('lets a fresh user save a verified provider entry without generating a pairing code',async()=>{
 const configure=vi.fn().mockResolvedValue({method:'TAILSCALE_FUNNEL',origin:'https://pc.tail1.ts.net',port:47114,configured:true});
 const api:WebAccessApi={devices:vi.fn().mockResolvedValue([]),url:vi.fn().mockResolvedValue(null),issue:vi.fn(),revoke:vi.fn(),connection:vi.fn().mockResolvedValue({method:'NONE',origin:null,port:47114,configured:false}),configure};
 render(<DesktopWebAccess api={api}/>);fireEvent.click(screen.getByRole('button',{name:'手机/网页登录'}));
 await screen.findByLabelText('HTTPS 网址');expect(screen.getByRole('button',{name:'验证并保存'})).toBeDisabled();
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
