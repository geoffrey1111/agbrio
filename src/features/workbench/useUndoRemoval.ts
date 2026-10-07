import {t as uiText} from "../../i18n";
import {useEffect,useRef,useState} from 'react';

/** Optimistic local removal with serialized persistence for each exact identity.
 * Undo may precede the delete acknowledgement; it still persists after it.
 */
export function useUndoRemoval<T>(identity:(item:T)=>string,persist:(item:T,removed:boolean)=>Promise<unknown>,onError:(message:string)=>void){
 const [overrides,setOverrides]=useState(new Map<string,{item:T;removed:boolean}>());
 const [undoItem,setUndoItem]=useState<T|null>(null);
 const intents=useRef(new Map<string,{item:T;removed:boolean;revision:number}>());
 const queues=useRef(new Map<string,Promise<unknown>>());
 const mounted=useRef(true),operations=useRef({identity,persist,onError});operations.current={identity,persist,onError};
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 useEffect(()=>{if(!undoItem)return;const timer=setTimeout(()=>setUndoItem(null),6500);return()=>clearTimeout(timer);},[undoItem]);
 function change(item:T,removed:boolean){
  const {identity,persist}=operations.current,id=identity(item),prior=intents.current.get(id);
  if(prior?.removed===removed)return;
  const revision=(prior?.revision??0)+1;
  intents.current.set(id,{item,removed,revision});
  setOverrides(old=>new Map(old).set(id,{item,removed}));
  if(removed)setUndoItem(item);else setUndoItem(null);
  const work=(queues.current.get(id)??Promise.resolve()).catch(()=>{}).then(()=>persist(item,removed));
  queues.current.set(id,work);
  void work.then(()=>{
   if(!mounted.current||intents.current.get(id)?.revision!==revision)return;
   // Return authority to the shared server list after its acknowledgement.
   intents.current.delete(id);setOverrides(old=>{const next=new Map(old);next.delete(id);return next;});
  },()=>{});
  void work.catch(()=>{
   if(!mounted.current||intents.current.get(id)?.revision!==revision)return;
   intents.current.set(id,{item,removed:!removed,revision});
   setOverrides(old=>new Map(old).set(id,{item,removed:!removed}));setUndoItem(null);
   operations.current.onError(removed?uiText("删除未完成，条目已恢复。"):uiText("撤销未完成，请重新打开列表检查。"));
  }).finally(()=>{if(queues.current.get(id)===work)queues.current.delete(id);});
 }
 function project(items:T[]){
  const result=items.filter(item=>!overrides.get(identity(item))?.removed);
  for(const [id,override] of overrides)if(!override.removed&&!result.some(item=>identity(item)===id))result.push(override.item);
  return result;
 }
 return {remove:(item:T)=>change(item,true),restore:(item:T)=>change(item,false),project,undoItem,undo:()=>{if(undoItem)change(undoItem,false);},dismiss:()=>setUndoItem(null),isRemoved:(item:T)=>overrides.get(identity(item))?.removed===true};
}
