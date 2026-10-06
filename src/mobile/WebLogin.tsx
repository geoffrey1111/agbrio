import {useVisibleViewport} from "../features/workbench/mobileViewport";
import { clearRoleReviewCache } from "../features/workbench/roleReviewDrafts";
import {BrandMark} from "../features/workbench/BrandMark";
import {readMobileSession,type WebSession} from "./startup";
import {createContext,useContext,useEffect,useRef,useState,type ReactNode} from "react";
type Session=WebSession;
export type WebLoginApi={session:()=>Promise<Session>;pair:(code:string,deviceName:string,remember:boolean)=>Promise<unknown>;logout:()=>Promise<unknown>};
async function authRequest<T>(path:string,body?:unknown):Promise<T>{
 const abort=new AbortController(),timer=setTimeout(()=>abort.abort(),10000);let response:Response;
 try{response=await fetch(`/v1/mobile/auth/${path}`,{method:body===undefined?"GET":"POST",credentials:"same-origin",cache:"no-store",signal:abort.signal,headers:{"content-type":"application/json"},...(body===undefined?{}:{body:JSON.stringify(body)})});
 if(!response.ok){const data=await response.json().catch(()=>null);throw new Error(data?.error??`HTTP ${response.status}`);}return await response.json();}finally{clearTimeout(timer);}
}
export const webLoginApi:WebLoginApi={session:readMobileSession,pair:(code,deviceName,remember)=>authRequest("pair",{code,deviceName,remember}),logout:()=>authRequest("logout",{})};
const explain=(error:unknown)=>{
 const text=String(error);
 if(text.includes("WEB_PAIRING_INVALID_OR_EXPIRED"))return "配对码不正确、已过期或已用过。请在电脑端生成新码后再试。";
 if(text.includes("WEB_DEVICE_LIMIT"))return "已达到 20 台设备上限。请在电脑端移除旧设备，再重新配对。";
 if(text.includes("WEB_AUTH_STORE"))return "电脑端无法保存登录。请在 Router 的网页登录设置中检查。";
 if(text.includes("Host is not allowed")||text.includes("Origin is not allowed"))return "网页地址与电脑端设置不一致。请使用 Router 电脑端显示的网址。";
 return "暂时无法连接 Router。请确认电脑上的 Router 正在运行，再重试。";
};
const WebSessionContext=createContext<{logout:()=>void;busy:boolean;available:boolean}|null>(null);
export function WebSessionLogout(){const session=useContext(WebSessionContext);return session?.available?<button type="button" className="v4-web-logout" disabled={session.busy} onClick={session.logout}>{session.busy?"正在退出…":"退出此设备"}</button>:null;}
export function WebLoginGate({children,api=webLoginApi}:{children:ReactNode;api?:WebLoginApi}){
 useVisibleViewport();
 const[session,setSession]=useState<Session|null>(null),[code,setCode]=useState(""),[remember,setRemember]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState<string|null>(null),[name,setName]=useState("");
 const pending=useRef(false),alive=useRef(true),revision=useRef(0);
 useEffect(()=>{alive.current=true;let disposed=false;async function check(){if(pending.current)return;const mark=revision.current;try{const next=await api.session();if(!disposed&&mark===revision.current){setSession(next);setError(null);}}catch(e){if(!disposed&&mark===revision.current)setError(explain(e));}}
  void check();const timer=setInterval(()=>void check(),15000);const invalid=()=>{revision.current++;if(!disposed)setSession({authenticated:false});};window.addEventListener("aiwr-login-required",invalid);
  const online=()=>void check();window.addEventListener("online",online);
  return()=>{disposed=true;alive.current=false;clearInterval(timer);window.removeEventListener("aiwr-login-required",invalid);window.removeEventListener("online",online);};
 },[api]);
 async function pair(){if(pending.current)return;pending.current=true;revision.current++;setBusy(true);setError(null);try{await api.pair(code,name,remember);if(alive.current){setCode("");setSession(await api.session());}}catch(e){if(alive.current)setError(explain(e));}finally{pending.current=false;if(alive.current)setBusy(false);}}
 async function logout(){if(pending.current)return;pending.current=true;revision.current++;setBusy(true);try{await api.logout();clearRoleReviewCache();if(alive.current){setSession({authenticated:false});setCode("");setError(null);}}catch(e){if(alive.current)setError(explain(e));}finally{pending.current=false;if(alive.current)setBusy(false);}}
 async function retry(){if(pending.current)return;pending.current=true;revision.current++;setBusy(true);try{const next=await api.session();if(alive.current){setSession(next);setError(null);}}catch(e){if(alive.current)setError(explain(e));}finally{pending.current=false;if(alive.current)setBusy(false);}}
 if(session===null)return <main className="v4-web-login"><div><p className="v4-meta r2-login-brand"><BrandMark size={40}/>Agbrio · Agent Bridge</p><h1>{error?"正在重新连接":"正在连接工作台"}</h1><p role="status">{error??"正在检查设备登录…"}</p><button type="button" className="v3-primary" disabled={busy} onClick={()=>void retry()}>{busy?"正在检查…":"重新检查连接"}</button>{error&&<p className="v4-meta">临时掉线无需重新配对，连接恢复后会自动继续。</p>}</div></main>;
 if(session?.authenticated)return <WebSessionContext.Provider value={{logout:()=>void logout(),busy,available:session.method!=="CLOUDFLARE"}}>{error&&<p className="v4-login-status" role="status">{error}</p>}{children}</WebSessionContext.Provider>;
 return <main className="v4-web-login"><div><p className="v4-meta r2-login-brand"><BrandMark size={40}/>Agbrio · Agent Bridge</p><h1>连接你的工作台</h1><p>在电脑端「设置 → 设备」生成配对码。</p>
  <form onSubmit={event=>{event.preventDefault();void pair();}}><label htmlFor="web-pairing-code">6 位配对码</label><input id="web-pairing-code" name="pairingCode" inputMode="numeric" autoComplete="one-time-code" pattern="[0-9]{6}" maxLength={6} placeholder="000000" value={code} onChange={event=>setCode(event.target.value.replace(/\D/g,"").slice(0,6))} disabled={busy} required/>
   <label className="v4-login-remember"><input type="checkbox" checked={remember} disabled={busy} onChange={event=>setRemember(event.target.checked)}/>记住这台设备 90 天</label>
   <details><summary>设备名称（可选）</summary><label htmlFor="web-device-name">方便在电脑端识别<input id="web-device-name" value={name} maxLength={40} disabled={busy} placeholder="例如：我的手机" onChange={event=>setName(event.target.value)}/></label></details>
   {error&&<p role="alert">{error}</p>}<button className="v3-primary" type="submit" disabled={busy||code.length!==6}>{busy?"正在连接…":"连接工作台"}</button>
  </form>{session===null&&<button className="v4-back-link" type="button" disabled={busy} onClick={()=>{void api.session().then(setSession).catch(e=>setError(explain(e)));}}>重新检查连接</button>}<p className="v4-meta">配对码 5 分钟内有效。</p>
 </div></main>;
}
