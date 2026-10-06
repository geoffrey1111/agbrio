import {afterEach,expect,it,vi} from "vitest";
import {act,cleanup,fireEvent,render,screen,within} from "@testing-library/react";
import {useState} from "react";
import {NativeWorkbench} from "../features/workbench/NativeWorkbench";
import {RelayBlockSelection} from "../features/workbench/RelayBlockSelection";
import {DesktopWebAccess,type WebAccessApi} from "../features/workbench/DesktopWebAccess";
import {getLanguage,getLanguagePreference,resolveLanguage,setLanguagePreference,t} from "./index";
import {LanguagePicker} from "./LanguagePicker";
import en from "./en.json";
import traditional from "./zh-TW.json";
import {startupText} from "./startupCopy";
afterEach(()=>{cleanup();localStorage.clear();setLanguagePreference("system");});
it("uses the primary system language and distinguishes Traditional Chinese",()=>{
 expect(resolveLanguage(["zh-Hant-HK","en"])).toBe("zh-TW");
 expect(resolveLanguage(["zh-Hans-CN"])).toBe("zh-CN");
 expect(resolveLanguage(["ja-JP","zh-CN"])).toBe("en");
});
it("changes every mounted surface without remounting reader state or changing routing",()=>{
 const select=vi.fn();function Draft(){const[value,setValue]=useState("设置\n通知\nunchanged instructions");return <textarea aria-label="private draft" value={value} onChange={e=>setValue(e.target.value)}/>;}
 render(<NativeWorkbench items={[{id:"exact-id",name:"通知",lifecycle:"ACTIVE"}]} draft={{value:""}} onDraftChange={()=>{}} onSelectWorkstream={select} surface="SETTINGS" selectedWorkstreamId="exact-id" bridgePanel={<Draft/>}/>);
 const draft=screen.getByRole("textbox",{name:"private draft",hidden:true});
 fireEvent.change(screen.getByRole("combobox",{name:"语言"}),{target:{value:"en"}});
 expect(screen.getByRole("combobox",{name:"Language"})).toHaveValue("en");
 expect(screen.getByRole("button",{name:"Notification preferences"})).toBeVisible();
 expect(screen.getByRole("textbox",{name:"private draft",hidden:true})).toBe(draft);
 expect(draft).toHaveValue("设置\n通知\nunchanged instructions");expect(select).not.toHaveBeenCalled();
 expect(localStorage.getItem("agbrio.language.v1")).toBe("en");expect(document.documentElement.lang).toBe("en");
});
it("keeps original content verbatim even when it equals an application phrase",()=>{
 setLanguagePreference("en");render(<RelayBlockSelection blocks={[{id:"source-id",kind:"INSTRUCTION",recommended:true,text:"设置\n\n通知\n\n确认发送"}]} selected={[]} disabled={false} onChange={()=>{}}/>);
 expect(screen.getByText("Instruction")).toBeVisible();expect(screen.queryByText("建议转发")).not.toBeInTheDocument();
 const content=document.querySelector(".markdown-message")!;expect(Array.from(content.querySelectorAll("p")).map(p=>p.textContent)).toEqual(["设置","通知","确认发送"]);
 expect(screen.getByRole("checkbox",{name:/Block 1/})).not.toBeChecked();
});
it("updates an existing failure and preserves the exact HTTPS input without retrying",async()=>{
 const configure=vi.fn().mockRejectedValue(Error("MOBILE_PROBE_FAILED"));const api:WebAccessApi={connection:async()=>({method:"NONE",configured:false,port:47114,origin:null}),configure,devices:async()=>[],url:async()=>null,issue:vi.fn(),revoke:vi.fn()};
 render(<><LanguagePicker/><DesktopWebAccess api={api}/></>);fireEvent.click(screen.getByRole("button",{name:"手机/网页登录"}));const input=await screen.findByRole("textbox",{name:"HTTPS 网址"});fireEvent.change(input,{target:{value:"https://exact.example"}});fireEvent.click(screen.getByRole("button",{name:"验证并保存"}));await screen.findByText("未能验证这个入口，原连接已保留。请检查转发和 HTTPS 配置。");
 act(()=>setLanguagePreference("en"));expect(screen.getByRole("alert")).toHaveTextContent("Could not verify this address");expect(screen.getByRole("textbox",{name:"HTTPS address"})).toBe(input);expect(input).toHaveValue("https://exact.example");expect(configure).toHaveBeenCalledTimes(1);
 act(()=>setLanguagePreference("zh-TW"));expect(screen.getByRole("alert")).toHaveTextContent("未能驗證");expect(api.issue).not.toHaveBeenCalled();
});
it("keeps explicit preference in sync even when it resolves to the same language",()=>{
 render(<LanguagePicker/>);fireEvent.change(screen.getByRole("combobox",{name:"语言"}),{target:{value:"zh-CN"}});expect(getLanguagePreference()).toBe("zh-CN");
 act(()=>{localStorage.setItem("agbrio.language.v1","en");window.dispatchEvent(new StorageEvent("storage",{key:"agbrio.language.v1"}));});expect(getLanguage()).toBe("en");expect(screen.getByRole("combobox",{name:"Language"})).toHaveValue("en");
});
it("has complete catalogs and preserves each interpolation slot",()=>{
 expect(Object.keys(en).sort()).toEqual(Object.keys(traditional).sort());
 for(const [source,value]of Object.entries(en)){expect(value.trim(),source).not.toBe("");expect(value.match(/\{\d+\}/g)?.sort()??[],source).toEqual(source.match(/\{\d+\}/g)?.sort()??[]);}
 setLanguagePreference("en");expect(t("转给{0}","exact recipient")).toBe("Forward to exact recipient");expect(t("unrecognized-provider-code")).toBe("unrecognized-provider-code");
});
it("uses the same boot-shell copy without loading the main UI",()=>{
 for(const language of ["en","zh-CN","zh-TW"] as const){setLanguagePreference(language);for(const key of ["Agbrio 正在连接","正在连接…","重试","连接暂时不可用"])expect(startupText(key)).toBe(t(key));}
});
