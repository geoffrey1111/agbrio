import {useEffect,useState} from 'react';
import {readModels} from './readModelCache';

// Device-scoped presentation memory: "the owner opened this result". It only
// folds finished rows and clears the neutral new-result dot. It is never a
// delivery ledger, never an answer and never removes an unanswered question.
const event='agbrio-results-seen';
const memory=new Map<string,[string,number][]>();
const storageKey=()=>`agbrio.seen-results.v1:${readModels.presentationScope()??'unscoped'}`;
function read(key:string):[string,number][]{
 try{const value=JSON.parse(localStorage.getItem(key)??'null');if(Array.isArray(value))return value.filter((v):v is [string,number]=>Array.isArray(v)&&typeof v[0]==='string'&&v[0].length<600&&typeof v[1]==='number').slice(-512);}catch{}
 return memory.get(key)??[];
}
/** Records that the owner opened `id` whose latest activity was at `at`. */
export function markResultSeen(id:string,at:number|null|undefined){
 const key=storageKey(),stamp=typeof at==='number'&&Number.isFinite(at)&&at>0?at:Date.now();
 const rows=read(key);const prior=rows.find(r=>r[0]===id);if(prior&&prior[1]>=stamp)return;
 const next=[...rows.filter(r=>r[0]!==id),[id,stamp] as [string,number]].slice(-512);memory.set(key,next);
 if(readModels.presentationScope())try{localStorage.setItem(key,JSON.stringify(next));}catch{}
 window.dispatchEvent(new Event(event));
}
/** `seen(id, at)` is true when the owner opened `id` at or after activity `at`. */
export function useSeenResults(){
 const [,rerender]=useState(0);
 useEffect(()=>{const update=()=>rerender(n=>n+1);window.addEventListener(event,update);window.addEventListener('storage',update);return()=>{window.removeEventListener(event,update);window.removeEventListener('storage',update);};},[]);
 const rows=new Map(read(storageKey()));
 return (id:string,at:number|null|undefined)=>{const stamp=rows.get(id);return stamp!==undefined&&stamp>=(typeof at==='number'&&at>0?at:0);};
}
