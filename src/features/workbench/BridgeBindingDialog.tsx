import {t as uiText,tc,useLanguage,getLanguage} from "../../i18n";
import * as Dialog from "@radix-ui/react-dialog";
import {useMemo,useState} from "react";
import {motion} from "motion/react";
import {useBackLayer} from "./navigationHistory";
import {ArrowUpDown,Check,ChevronLeft,ChevronRight,RefreshCw,Search,X} from "lucide-react";
import {ClaudeSessionPicker,providerLabel,type ClaudePickerApi} from "./ClaudeSessionPicker";
import {ConversationAvatar} from "./ConversationHome";
import {activityTime,groupThreads,type CatalogWindow} from "../codex/threadCatalog";
import type {RoleBridgeApi,RoleInput} from "./RoleBridgePanel";
type Props={claudeApi?:ClaudePickerApi;workstreamName?:string;decision:RoleInput;execution:RoleInput;setDecision:(value:RoleInput)=>void;setExecution:(value:RoleInput)=>void;step:0|1|2;setStep:(step:0|1|2)=>void;catalog:Awaited<ReturnType<RoleBridgeApi["threads"]>>|null;busy:boolean;error:string|null;canSave:boolean;refresh:()=>void;close:()=>void;save:()=>void};
const roles=["控制端","执行端"];
const identity=(input:RoleInput)=>{try{const match=new URL(input.externalId).pathname.match(/\/c\/([^/]+)/);return match?match[1]:input.externalId.trim();}catch{return input.externalId.trim();}};
const avatarKey=(input:RoleInput)=>input.provider==="CLAUDE_CODE"?`w:claude_code:${identity(input)}`:input.provider==="CODEX"?`w:${identity(input)}`:`chatgpt:${identity(input)}`;
const avatarKind=(input:RoleInput)=>input.provider==="CLAUDE_CODE"?"claude":"codex";
const spring={type:"spring",stiffness:520,damping:32,mass:.7} as const;
const when=(seconds?:number|null)=>{if(!seconds)return "";const minutes=(Date.now()/1000-seconds)/60;if(minutes<1)return uiText("刚刚");if(minutes<60)return tc("{0} 分钟",Math.floor(minutes));const date=new Date(seconds*1000);return minutes<1440?new Intl.DateTimeFormat(getLanguage(),{hour:"2-digit",minute:"2-digit"}).format(date):new Intl.DateTimeFormat(getLanguage(),{month:"numeric",day:"numeric"}).format(date);};
const folder=(path?:string|null)=>path?.split(/[\\/]/).filter(Boolean).at(-1)??"";

/** Choose or change both ends of a Bridge: an overview of the two ends, and a
 * tap-to-choose list per end. Nothing is sent; saving only binds. */
export function BridgeBindingDialog({claudeApi,workstreamName,decision,execution,setDecision,setExecution,catalog,busy,error,canSave,refresh,close,save}:Props){
 useLanguage();
 const[editing,setEditing]=useState<number|null>(null),[query,setQuery]=useState(""),[range,setRange]=useState<CatalogWindow>("1");
 // Codex: choose a project first, then its conversations by recency, so the full catalog is never one long list.
 const[project,setProject]=useState<string|null>(null);
 useBackLayer(editing!==null,()=>setEditing(null));
 const input=editing===0?decision:execution,update=editing===0?setDecision:setExecution;
 const same=decision.provider===execution.provider&&identity(decision)===identity(execution);
 const valid=canSave&&Boolean(identity(decision)&&identity(execution))&&!same;
 const other=editing===0?execution:decision;
 const now=Date.now()/1000,all=catalog?.threads??[];
 const projects=useMemo(()=>groupThreads(all,"ALL","","ALL",now),[catalog]);
 const searching=query.trim().length>0;
 const threads=useMemo(()=>project===null&&!searching?[]:groupThreads(all,searching&&project===null?"ALL":range,query,project??"ALL",now).flatMap(g=>g.threads),[catalog,project,range,query,searching]);
 function openProject(key:string){
  // Start from the shortest window that has conversations, so a quiet project never opens empty.
  const windows:CatalogWindow[]=["1","3","7","30","ALL"];
  setRange(windows.find(w=>groupThreads(all,w,"",key,now).some(g=>g.threads.length))??"ALL");setProject(key);setQuery("");
 }
 function choose(next:RoleInput){update(next);setEditing(null);setQuery("");setProject(null);}
 const title=editing===null?workstreamName||uiText("绑定两端"):uiText("选择{0}", uiText(roles[editing]));
 return <Dialog.Root open onOpenChange={open=>{if(!open&&!busy)close();}}><Dialog.Portal><Dialog.Overlay className="v6-scrim"/><Dialog.Content className="v6-sheet v6-binding" aria-describedby={undefined}>
  <span className="v6-grab" aria-hidden/>
  <header className="v6-sheet-head"><button type="button" className="v6-icon" aria-label={editing===null?uiText("关闭绑定"):uiText("返回两端")} disabled={busy} onClick={()=>{if(editing===null)close();else{setEditing(null);setProject(null);setQuery("");}}}>{editing===null?<X size={20}/>:<ChevronLeft size={22}/>}</button><Dialog.Title>{title}</Dialog.Title></header>
  {editing===null?<motion.div key="overview" className="v6-step" initial={{opacity:0,x:-20}} animate={{opacity:1,x:0}} transition={spring}>
   <p className="v6-muted">{identity(decision)&&identity(execution)?uiText("点一端更换对话。保存只改变绑定，不会发送任何内容。"):uiText("选好控制端和执行端。保存只改变绑定，不会发送任何内容。")}</p>
   <div className="v6-binding-ends">{([decision,execution] as const).map((endpoint,index)=><motion.button key={index} type="button" className="v6-row" disabled={busy} aria-label={uiText("选择{0}", uiText(roles[index]))} onClick={()=>setEditing(index)} whileTap={{scale:.98}}>
    <span className="v6-binding-role">{uiText(roles[index])}</span>
    {identity(endpoint)?<ConversationAvatar id={avatarKey(endpoint)} kind={avatarKind(endpoint)}/>:<span className="v6-avatar v6-avatar-empty" aria-hidden/>}
    <span className="v6-row-body"><span className="v6-row-top"><strong>{endpoint.label||uiText("选择对话")}</strong></span><span className="v6-row-meta">{identity(endpoint)?providerLabel(endpoint.provider):uiText("尚未选择")}</span></span><ChevronRight size={18} className="v6-chevron"/>
   </motion.button>)}</div>
   <button type="button" className="v6-swap" aria-label={uiText("交换两端")} disabled={busy} onClick={()=>{setDecision(execution);setExecution(decision);}}><ArrowUpDown size={15}/>{uiText("交换两端")}</button>
   {same&&identity(decision)&&<p role="alert" className="v6-error">{uiText("两端请选择不同的对话。")}</p>}{error&&<p role="alert" className="v6-error">{uiText(error)}</p>}
   <div className="v6-sheet-foot"><button type="button" className="v6-primary" disabled={busy||!valid} onClick={save}><Check size={18}/>{busy?uiText("保存中…"):uiText("保存两端")}</button></div>
  </motion.div>:<motion.div key={`edit-${editing}`} className="v6-step" initial={{opacity:0,x:20}} animate={{opacity:1,x:0}} transition={spring}>
   <div className="v6-binding-edit" aria-busy={busy}>
    <div className="v6-seg" role="group" aria-label={uiText("选择{0}", uiText(roles[editing]))}>{(["CODEX","CLAUDE_CODE","CHATGPT"] as const).map(provider=><button key={provider} type="button" disabled={busy} aria-pressed={provider===input.provider} onClick={()=>{if(provider!==input.provider)update({provider,externalId:"",label:""});}}>{providerLabel(provider)}</button>)}</div>
    {input.provider==="CODEX"&&<>
     <label className="v6-search"><Search size={16} aria-hidden/><input type="search" aria-label={uiText("{0}搜索对话或项目", uiText(roles[editing]))} placeholder={project===null?uiText("搜索全部对话"):uiText("在这个项目里搜索")} value={query} onChange={e=>setQuery(e.target.value)}/></label>
     {project!==null&&<div className="v6-chips"><button type="button" className="v6-chip-back" aria-label={uiText("返回项目列表")} onClick={()=>{setProject(null);setQuery("");}}><ChevronLeft size={14}/>{project==="ALL"?uiText("全部项目"):projects.find(g=>g.key===project)?.label??uiText("项目")}</button>{([["1",uiText("1 天")],["3",uiText("3 天")],["7",uiText("7 天")],["30",uiText("30 天")],["ALL",uiText("全部")]] as const).map(([id,label])=><button key={id} type="button" aria-pressed={range===id} onClick={()=>setRange(id)}>{label}</button>)}</div>}
     <section className="v6-sheet-scroll" aria-label={uiText("{0} Codex 对话", uiText(roles[editing]))}>
      {catalog===null?<p className="v6-muted" role="status">{uiText("正在读取电脑上的对话…")}</p>
      :project===null&&!searching?<>
       <button type="button" className="v6-row v6-project-row" onClick={()=>openProject("ALL")}><span className="v6-project-glyph" aria-hidden>∗</span><span className="v6-row-body"><span className="v6-row-top"><strong>{uiText("全部项目")}</strong></span><span className="v6-row-meta">{tc("{0} 条对话",all.length)}</span></span><ChevronRight size={18} className="v6-chevron"/></button>
       {projects.map(g=>{const latest=activityTime(g.threads[0]);return <button key={g.key} type="button" className="v6-row v6-project-row" onClick={()=>openProject(g.key)}><span className="v6-project-glyph" aria-hidden>{g.label.slice(0,1)}</span><span className="v6-row-body"><span className="v6-row-top"><strong>{g.label}</strong>{latest?<time>{when(latest)}</time>:null}</span><span className="v6-row-meta">{tc("{0} 条对话",g.threads.length)}</span></span><ChevronRight size={18} className="v6-chevron"/></button>;})}
      </>
      :threads.length?threads.map((t,i)=><motion.button key={t.id} type="button" className="v6-row" data-chosen={t.id===input.externalId} disabled={(other.provider==="CODEX"&&identity(other)===t.id)||busy} onClick={()=>choose({...input,externalId:t.id,label:t.label})} whileTap={{scale:.98}} initial={{opacity:.35,y:8}} animate={{opacity:1,y:0}} transition={{...spring,delay:Math.min(i,12)*.02}}>
        <ConversationAvatar id={`w:${t.id}`} kind="codex"/><span className="v6-row-body"><span className="v6-row-top"><strong>{t.label}</strong>{t.recencyAt?<time>{when(t.recencyAt)}</time>:null}</span><span className="v6-row-meta">{t.projectLabel||folder(t.projectProvenance)||uiText("无项目")}{other.provider==="CODEX"&&identity(other)===t.id?` · ${uiText("已是另一端")}`:t.id===input.externalId?` · ${uiText("当前")}`:""}</span></span>
       </motion.button>)
      :<div className="v6-muted"><p>{searching?uiText("没有找到匹配的对话。"):uiText("这个时间范围里没有对话。")}</p>{!searching&&range!=="ALL"&&<button type="button" className="v6-swap" onClick={()=>setRange("ALL")}>{uiText("查看全部时间")}</button>}</div>}
      <p className="v6-muted v6-catalog-status">{catalog?(catalog.complete?tc("已扫描全部 {0} 条未归档对话", all.length):tc("已扫描 {0} 条，目录尚未完整", all.length)):""}</p>
     </section>
     <button type="button" className="v6-swap" onClick={refresh}><RefreshCw size={14}/>{uiText("刷新目录")}</button>
    </>}
    {input.provider==="CLAUDE_CODE"&&<div className="v6-sheet-scroll"><ClaudeSessionPicker api={claudeApi??{}} value={input} onChange={next=>choose(next)}/></div>}
    {input.provider==="CHATGPT"&&<><label className="v6-field">{uiText("对话链接")}<input aria-label={uiText("{0} ChatGPT 对话链接", uiText(roles[editing]))} value={input.externalId} placeholder="https://chatgpt.com/c/…" onChange={event=>update({...input,externalId:event.target.value,label:input.label||uiText("ChatGPT 对话")})}/></label>
     <div className="v6-sheet-foot"><button type="button" className="v6-primary" disabled={busy||!identity(input)} onClick={()=>setEditing(null)}><Check size={18}/>{uiText("使用此对话")}</button></div></>}
   </div>
  </motion.div>}
 </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
