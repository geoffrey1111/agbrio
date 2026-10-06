import {t as uiText,tc,useLanguage} from "../../i18n";
import { useState } from "react";
import { ArrowRight, Plus, Search } from "lucide-react";
import type { WorkbenchItem } from "./models";
export function BridgeDirectory({items,onSelect,onNew}:{items:WorkbenchItem[];onSelect:(id:string)=>void;onNew:()=>void}){
 useLanguage();
 const [query,setQuery]=useState("");const active=items.filter(i=>i.lifecycle==="ACTIVE"&&`${i.name} ${i.projectName??""} ${i.sourceLabel??""}`.toLowerCase().includes(query.toLowerCase()));
 return <section className="v5-bridge-directory" aria-label={uiText("Bridge 列表")}><header><h1>Bridge</h1><button className="v3-primary" onClick={onNew}><Plus size={18}/>{uiText("新建")}</button></header><label className="v5-search"><Search size={18}/><input aria-label={uiText("搜索 Bridge")} placeholder={uiText("搜索")} value={query} onChange={e=>setQuery(e.target.value)}/></label><div>{active.map(item=><button key={item.id} className="v5-bridge-row" onClick={()=>onSelect(item.id)}><span className="v5-bridge-card-content"><strong>{item.name}</strong><small title={item.sourceLabel||item.projectName||uiText("未绑定")}>{item.sourceLabel||item.projectName||uiText("未绑定")}</small><span className="v5-bridge-attention">{item.attentionCount?tc("{0} 项待办", item.attentionCount):uiText("暂无待办")}</span><span className="v5-bridge-open">{uiText("打开")}<ArrowRight size={18}/></span></span></button>)}</div>{!active.length&&<p className="v4-meta">{query?uiText("没有匹配的 Bridge"):uiText("还没有 Bridge")}</p>}</section>;
}
export function NewBridgeForm({create,cancel}:{create:(name:string)=>Promise<void>;cancel:()=>void}){
 useLanguage();
 const[name,setName]=useState("");const[busy,setBusy]=useState(false);const[error,setError]=useState("");
 return <section className="v5-new-bridge"><h1>{uiText("新建 Bridge")}</h1><form onSubmit={e=>{e.preventDefault();if(busy)return;setBusy(true);void create(name.trim()||uiText("新 Bridge")).catch(e=>setError(String(e))).finally(()=>setBusy(false));}}><label>{uiText("名称")}<input autoFocus value={name} maxLength={100} onChange={e=>setName(e.target.value)} placeholder={uiText("例如：数值研究")} disabled={busy}/></label>{error&&<p role="alert">{uiText(error)}</p>}<footer><button type="button" disabled={busy} onClick={cancel}>{uiText("取消")}</button><button className="v3-primary" disabled={busy}>{busy?uiText("创建中…"):uiText("选择两端")}</button></footer></form></section>;
}
