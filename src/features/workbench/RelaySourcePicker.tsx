import {useState} from "react";
import {Check,History} from "lucide-react";
import {t,tc,getLanguage} from "../../i18n";
import {messagePreview} from "./messagePreview";
export type RelaySource={id:string;text:string;observedAt?:number;completedAt?:number|null};
/** Pick an exact retained observation; never rank business value or merge turns. */
export function RelaySourcePicker({sources,selected,disabled,stopped,onChange}:{sources:RelaySource[];selected:string;disabled:boolean;stopped:boolean;onChange:(id:string)=>void}){
 const[open,setOpen]=useState(stopped),[limit,setLimit]=useState(10);
 if(sources.length<2)return null;
 return <section className="r2-relay-sources" aria-label={t("来源回复")}>
  <button type="button" className="r2-source-toggle" aria-expanded={open} disabled={disabled} onClick={()=>setOpen(!open)}><History size={16}/><span>{stopped?t("目标已停滞"):t("来源回复")}</span><small>{tc("{0} 条回复",sources.length)}</small></button>
  {open&&<div className="r2-source-list" role="group" aria-label={t("选择要转发的回复")}>{sources.slice(-limit).map((source,index)=>{
   const chosen=source.id===selected,time=source.completedAt??source.observedAt,ordinal=Math.max(0,sources.length-limit)+index+1;
   return <button key={source.id} type="button" aria-pressed={chosen} disabled={disabled} onClick={()=>{onChange(source.id);setOpen(false);}}><span className="r2-source-check">{chosen?<Check size={16}/>:ordinal}</span><span><small>{time?new Intl.DateTimeFormat(getLanguage(),{month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"}).format(time):t("已保存")}{source.id===sources.at(-1)?.id?` · ${t("最新回复")}`:""}</small><strong>{messagePreview(source.text).slice(0,180)}</strong></span></button>;
  })}{sources.length>limit&&<button type="button" onClick={()=>setLimit(limit+10)}>{t("查看此前回复 · {0}",sources.length-limit)}</button>}</div>}
 </section>;
}
