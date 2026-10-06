import { useState } from "react";
import { ArrowRight, Plus, Search } from "lucide-react";
import type { WorkbenchItem } from "./models";
export function BridgeDirectory({items,onSelect,onNew}:{items:WorkbenchItem[];onSelect:(id:string)=>void;onNew:()=>void}){
 const [query,setQuery]=useState("");const active=items.filter(i=>i.lifecycle==="ACTIVE"&&`${i.name} ${i.projectName??""} ${i.sourceLabel??""}`.toLowerCase().includes(query.toLowerCase()));
 return <section className="v5-bridge-directory" aria-label="Bridge 列表"><header><h1>Bridge</h1><button className="v3-primary" onClick={onNew}><Plus size={18}/>新建</button></header><label className="v5-search"><Search size={18}/><input aria-label="搜索 Bridge" placeholder="搜索" value={query} onChange={e=>setQuery(e.target.value)}/></label><div>{active.map(item=><button key={item.id} className="v5-bridge-row" onClick={()=>onSelect(item.id)}><span className="v5-bridge-card-content"><strong>{item.name}</strong><small title={item.sourceLabel||item.projectName||"未绑定"}>{item.sourceLabel||item.projectName||"未绑定"}</small><span className="v5-bridge-attention">{item.attentionCount?`${item.attentionCount} 项待办`:"暂无待办"}</span><span className="v5-bridge-open">打开<ArrowRight size={18}/></span></span></button>)}</div>{!active.length&&<p className="v4-meta">{query?"没有匹配的 Bridge":"还没有 Bridge"}</p>}</section>;
}
export function NewBridgeForm({create,cancel}:{create:(name:string)=>Promise<void>;cancel:()=>void}){
 const[name,setName]=useState("");const[busy,setBusy]=useState(false);const[error,setError]=useState("");
 return <section className="v5-new-bridge"><h1>新建 Bridge</h1><form onSubmit={e=>{e.preventDefault();if(busy)return;setBusy(true);void create(name.trim()||"新 Bridge").catch(e=>setError(String(e))).finally(()=>setBusy(false));}}><label>名称<input autoFocus value={name} maxLength={100} onChange={e=>setName(e.target.value)} placeholder="例如：数值研究" disabled={busy}/></label>{error&&<p role="alert">{error}</p>}<footer><button type="button" disabled={busy} onClick={cancel}>取消</button><button className="v3-primary" disabled={busy}>{busy?"创建中…":"选择两端"}</button></footer></form></section>;
}
