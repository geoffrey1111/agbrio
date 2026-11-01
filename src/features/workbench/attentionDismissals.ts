import {useEffect,useState} from 'react';
import {readModels} from './readModelCache';

// Device/session-scoped presentation preferences, never a delivery ledger.
// Store stable identities only: no message bodies, commands or credentials.
const event='agbrio-attention-dismissed';
const memory=new Map<string,string[]>();
const storageKey=()=>`agbrio.dismissed-attention.v1:${readModels.presentationScope()??'unscoped'}`;
function read(key:string){
 try{const value=JSON.parse(localStorage.getItem(key)??'null');if(Array.isArray(value))return value.filter((v):v is string=>typeof v==='string'&&v.length<1500).slice(-256);}catch{}
 return memory.get(key)??[];
}
export function dismissAttention(id:string){const key=storageKey();const rows=[...read(key).filter(v=>v!==id),id].slice(-256);memory.set(key,rows);if(readModels.presentationScope())try{localStorage.setItem(key,JSON.stringify(rows));}catch{}window.dispatchEvent(new Event(event));}
export function useDismissedAttention(){const [,rerender]=useState(0);useEffect(()=>{const update=()=>rerender(n=>n+1);window.addEventListener(event,update);window.addEventListener('storage',update);return()=>{window.removeEventListener(event,update);window.removeEventListener('storage',update);};},[]);const rows=read(storageKey());return (id:string)=>rows.includes(id);}
export const replyAttentionKey=(threadId:string,id:string)=>JSON.stringify(['reply',threadId,id]);
export const decisionAttentionKey=(threadId:string,childId:string|undefined,id:string,revision:number)=>JSON.stringify(['decision',threadId,childId??null,id,revision]);
export function failedAttentionKey(w:{threadId:string;generation:number;lastMessageAt?:number|null;changedAt?:number|null;snapshot:{state:string;turnId:string|null;itemId:string|null}}){return JSON.stringify(['failure',w.threadId,w.generation,w.snapshot.state,w.snapshot.turnId,w.snapshot.itemId,w.lastMessageAt??null,w.changedAt??null]);}
