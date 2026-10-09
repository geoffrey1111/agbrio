import{useLayoutEffect,useMemo,useState}from'react';
import type{CodexWatch}from'./CodexNotifications';
const byUsage=(a:CodexWatch,b:CodexWatch)=>(b.sendCount??0)-(a.sendCount??0)||(a.threadId<b.threadId?-1:a.threadId>b.threadId?1:0);
/** One notification WATCHES visit includes its opened chat and return to list. */
export function useWatchVisitOrder(rows:CodexWatch[],active:boolean){
 const[order,setOrder]=useState<string[]>([]);
 const ids=rows.map(row=>row.threadId).join('\n');
 useLayoutEffect(()=>{
  if(!active){setOrder(old=>old.length?[]:old);return;}
  setOrder(old=>{const added=rows.filter(row=>!old.includes(row.threadId)).sort(byUsage).map(row=>row.threadId);return added.length?[...old,...added]:old;});
 },[active,ids]);
 return useMemo(()=>{if(!active||!order.length)return [...rows].sort(byUsage);const rank=new Map(order.map((id,index)=>[id,index]));return [...rows].sort((a,b)=>(rank.get(a.threadId)??Infinity)-(rank.get(b.threadId)??Infinity)||byUsage(a,b));},[rows,order,active]);
}
