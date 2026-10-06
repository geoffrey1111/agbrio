import { useMemo,useState } from "react";
import type { ExistingCodexThreadCandidate } from "./types";
import {groupThreads,projectLabel,activityLabel,activityTime,type CatalogWindow} from "./threadCatalog";
export function CodexThreadPicker({threads,complete=true,value,onChange,ariaLabel,searchLabel,disabled=false,currentLabel,mode="select"}:{threads:ExistingCodexThreadCandidate[];complete?:boolean;value:string;onChange:(id:string)=>void;ariaLabel:string;searchLabel?:string;disabled?:boolean;currentLabel?:string;mode?:"select"|"list"}){
 const [range,setRange]=useState<CatalogWindow>("1"),[query,setQuery]=useState(""),[project,setProject]=useState("ALL");
 const groups=useMemo(()=>groupThreads(threads,range,query,project,Date.now()/1000),[threads,range,query,project]);
 const projects=useMemo(()=>groupThreads(threads,"ALL","","ALL",Date.now()/1000),[threads]);
 const count=groups.reduce((sum,g)=>sum+g.threads.length,0);const selected=threads.find(t=>t.id===value);const visible=groups.some(g=>g.threads.some(t=>t.id===value));
 return <section className="v3-thread-picker" aria-label={`${ariaLabel}目录`}>
  <div className="v3-thread-picker-filters"><label>最近活动<select aria-label={`${ariaLabel}时间范围`} value={range} disabled={disabled} onChange={e=>setRange(e.target.value as CatalogWindow)}><option value="1">最近 1 天</option><option value="3">最近 3 天</option><option value="7">最近 7 天</option><option value="30">最近 30 天</option><option value="ALL">全部</option></select></label><label>项目<select aria-label={`${ariaLabel}项目`} value={project} disabled={disabled} onChange={e=>setProject(e.target.value)}><option value="ALL">全部项目</option>{projects.map(g=><option key={g.key} value={g.key}>{g.label} · {g.threads.length} 条</option>)}</select></label></div>
  <label>搜索对话或项目<input aria-label={searchLabel??`${ariaLabel}搜索对话或项目`} disabled={disabled} value={query} onChange={e=>setQuery(e.target.value)}/></label>
  <p className="v3-thread-picker-status" role="status">{complete?`已扫描全部 ${threads.length} 条未归档对话`:`已扫描 ${threads.length} 条，目录尚未完整`} · 当前显示 {count} 条</p>
  {threads.some(t=>activityTime(t)===null)&&<p>缺少最近活动时间的对话可在「全部」查看。</p>}
  {mode==="select"?<label>对话名称<select aria-label={ariaLabel} value={value} disabled={disabled} onChange={e=>onChange(e.target.value)}><option value="">选择现有对话</option>{value&&!visible&&<option value={value}>{selected?.label||currentLabel||"当前绑定"} · 当前选择，筛选范围外</option>}{groups.map(g=><optgroup key={g.key} label={`${g.label} · ${g.threads.length} 条`}>{g.threads.map(t=><option key={t.id} value={t.id}>{t.label} · {activityLabel(t)}</option>)}</optgroup>)}</select></label>:<div className="v3-thread-picker-groups">{groups.map(g=><section key={g.key} aria-label={g.label}><h3>{g.label} · {g.threads.length} 条</h3>{g.threads.map(t=><button type="button" className="candidate" key={t.id} disabled={disabled} onClick={()=>onChange(t.id)}><strong>{t.label}</strong>{t.preview&&t.preview!==t.label&&<small>{t.preview.slice(0,160)}</small>}<small>最近活动：{activityLabel(t)}</small></button>)}</section>)}</div>}
  {count===0&&threads.length>0&&<p>当前筛选没有匹配的对话。可扩大时间范围或清除搜索。</p>}
  {selected&&mode==="select"&&<div className="v3-thread-picker-selected"><strong>{selected.label}</strong><p>{projectLabel(selected)} · 最近活动：{activityLabel(selected)}</p></div>}
 </section>;
}
