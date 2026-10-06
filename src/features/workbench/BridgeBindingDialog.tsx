import {t as uiText,useLanguage} from "../../i18n";
import * as Dialog from "@radix-ui/react-dialog";
import {useState} from "react";
import {useBackLayer} from "./navigationHistory";
import {ArrowRightLeft,Check,ChevronDown,ChevronLeft,Link2,RefreshCw} from "lucide-react";
import {CodexThreadPicker} from "../codex/CodexThreadPicker";
import type {RoleBridgeApi,RoleInput} from "./RoleBridgePanel";
type Props={workstreamName?:string;decision:RoleInput;execution:RoleInput;setDecision:(value:RoleInput)=>void;setExecution:(value:RoleInput)=>void;step:0|1|2;setStep:(step:0|1|2)=>void;catalog:Awaited<ReturnType<RoleBridgeApi["threads"]>>|null;busy:boolean;error:string|null;canSave:boolean;refresh:()=>void;close:()=>void;save:()=>void};
const roles=["控制端","执行端"];
const identity=(input:RoleInput)=>{try{const match=new URL(input.externalId).pathname.match(/\/c\/([^/]+)/);return match?match[1]:input.externalId.trim();}catch{return input.externalId.trim();}};
export function BridgeBindingDialog({workstreamName,decision,execution,setDecision,setExecution,catalog,busy,error,canSave,refresh,close,save}:Props){
 useLanguage();
 const[editing,setEditing]=useState<number|null>(null);
 useBackLayer(editing!==null,()=>setEditing(null));
 const input=editing===0?decision:execution,update=editing===0?setDecision:setExecution;
 const same=decision.provider===execution.provider&&identity(decision)===identity(execution);
 const valid=canSave&&Boolean(identity(decision)&&identity(execution))&&!same;
 return <Dialog.Root open onOpenChange={open=>{if(!open&&!busy)close();}}><Dialog.Portal><Dialog.Overlay className="v3-role-dialog-overlay"/><Dialog.Content className="v3-role-dialog v5-binding-dialog r2-binding" aria-describedby={undefined}>
  <header className="r2-binding-header"><button type="button" className="r2-icon" aria-label={editing===null?uiText("关闭绑定"):uiText("返回两端")} disabled={busy} onClick={()=>editing===null?close():setEditing(null)}><ChevronLeft size={20}/></button><Dialog.Title>{editing===null?workstreamName||uiText("绑定两端"):uiText("选择{0}", uiText(roles[editing]))}</Dialog.Title><button type="button" className="r2-icon" aria-label={uiText("交换两端")} disabled={busy||editing!==null} onClick={()=>{setDecision(execution);setExecution(decision);}}><ArrowRightLeft size={20}/></button></header>
  <div className="v5-binding-body">
   {editing===null?<div className="r2-binding-endpoints"><h3>{identity(decision)&&identity(execution)?uiText("更换两端"):uiText("绑定两端")}</h3>{([decision,execution]as const).map((endpoint,index)=><section key={index}><small>{uiText(roles[index])}</small><button type="button" disabled={busy} aria-label={uiText("选择{0}", uiText(roles[index]))} onClick={()=>setEditing(index)}><Link2 size={20}/><span><strong>{endpoint.label||uiText("选择对话")}</strong><small>{endpoint.provider==="CODEX"?"Codex":"ChatGPT"}</small></span><ChevronDown size={18}/></button></section>)}</div>:<fieldset disabled={busy} aria-label={uiText("选择{0}", uiText(roles[editing]))}><div className="v4-provider-choice">{(["CODEX","CHATGPT"]as const).map(provider=><button key={provider} type="button" aria-pressed={provider===input.provider} onClick={()=>{if(provider!==input.provider)update({provider,externalId:"",label:""});}}>{provider==="CODEX"?"Codex":"ChatGPT"}</button>)}</div>
   {input.provider==="CODEX"?<><CodexThreadPicker threads={catalog?.threads??[]} complete={catalog?.complete??false} value={input.externalId} currentLabel={input.label} ariaLabel={uiText("{0} Codex 对话", uiText(roles[editing]))} onChange={id=>{const thread=catalog?.threads.find(item=>item.id===id);update({...input,externalId:id,label:thread?.label??input.label});}}/><button type="button" disabled={busy} onClick={refresh}><RefreshCw size={18}/>{uiText("刷新目录")}</button></>:<label>{uiText("对话链接")}<input aria-label={uiText("{0} ChatGPT 对话链接", uiText(roles[editing]))} value={input.externalId} placeholder="https://chatgpt.com/c/…" onChange={event=>update({...input,externalId:event.target.value,label:input.label||uiText("ChatGPT 对话")})}/></label>}</fieldset>}
   {same&&identity(decision)&&<p role="alert">{uiText("两端请选择不同的对话。")}</p>}{error&&<p role="alert">{uiText(error)}</p>}
  </div>
  <footer>{editing===null?<button type="button" className="v3-primary r2-action" disabled={busy||!valid} onClick={save}><Check size={18}/>{busy?uiText("保存中…"):uiText("保存两端")}</button>:<button type="button" className="v3-primary r2-action" disabled={busy||!identity(input)} onClick={()=>setEditing(null)}><Check size={18}/>{uiText("使用此对话")}</button>}</footer>
 </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
