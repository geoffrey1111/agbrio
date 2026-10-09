import type {ChatMessage,WatchReply} from './watchChatApi';
export type ObservedMessage=ChatMessage&{seenAt:number};
export type TimelineMessage=ChatMessage&{receipt?:WatchReply;source?:boolean};
const identity=(m:ChatMessage)=>JSON.stringify([m.turnId,m.id]);
/** The Host returns newest turns first, but items inside each turn oldest first. */
export function chronologicalHistoryPage(messages:ChatMessage[]){
 const groups=new Map<string,ChatMessage[]>();
 for(const message of messages){const group=groups.get(message.turnId)??[];group.push(message);groups.set(message.turnId,group);}
 return [...groups.values()].reverse().flat();
}
export function mergeHistory(older:ChatMessage[],newer:ChatMessage[]){
 const groups=new Map<string,ChatMessage[]>();
 for(const message of older){const group=groups.get(message.turnId)??[];group.push(message);groups.set(message.turnId,group);}
 const incoming=new Map<string,ChatMessage[]>();
 for(const message of newer){const group=incoming.get(message.turnId)??[];group.push(message);incoming.set(message.turnId,group);}
 for(const [turn,page] of incoming){
  const ids=new Set(page.map(identity));
  const previous=groups.get(turn)??[];
  groups.set(turn,[...previous.filter(m=>!ids.has(identity(m))),...page]);
 }
 return [...groups.values()].flat();
}
/** Native turn/item order owns the transcript. Local receipts decorate matching
 * user items once; they are not a second list of all the user's sent messages.
 * Sent receipts outside the loaded native window reappear when older pages load.
 * Historical unsent/acknowledged attempts are delivery records, not new chat
 * messages. Only attempts handled in this reader visit stay as standalone rows.
 */
export function chatTimeline(history:ChatMessage[],observed:ObservedMessage[],replies:WatchReply[],source:ObservedMessage,loaded:boolean,current?:ChatMessage,sourceIsOlder=false,currentAttemptIds:ReadonlySet<string>=new Set()):TimelineMessage[]{
 const nativeTurns=new Set(history.map(m=>m.turnId));
 const groups=new Map<string,TimelineMessage[]>();
 const times=new Map<string,number>();
 const add=(message:TimelineMessage)=>{const group=groups.get(message.turnId)??[];const index=group.findIndex(m=>m.id===message.id);if(index<0)group.push(message);else group[index]={...group[index],...message};groups.set(message.turnId,group);};
 for(const message of history)add(message);
 for(const message of [...observed,source]){
  times.set(message.turnId,Math.min(times.get(message.turnId)??Infinity,message.seenAt));
  const current=groups.get(message.turnId)?.find(m=>m.id===message.id);
  if(current)current.source=identity(message)===identity(source);
  else add({...message,source:identity(message)===identity(source)});
 }
 // The current live snapshot can advance inside the same native item. Older
 // session-cache text must never overwrite a freshly read native message.
 if(current)add({...current,source:identity(current)===identity(source)});
 const sorted=[...replies].sort((a,b)=>a.createdAt-b.createdAt);
 for(const receipt of sorted){
  const turn=receipt.turnId??`receipt:${receipt.id}`;
  const group=groups.get(turn)??[];
  const match=group.find(m=>m.role==='user'&&m.text===receipt.text&&!m.receipt)??
   // SEND/QUEUE's acknowledged turn is native identity for its initiating prompt.
   // Native text may also contain the appended material manifest. STEER can
   // contain several prompts in one turn and must not use this fallback.
   (receipt.mode!=='STEER'&&receipt.status==='SENT'?group.find(m=>m.role==='user'&&!m.receipt):undefined);
  if(match){match.receipt=receipt;continue;}
  if(['CANCELLED','FAILED','ACKNOWLEDGED'].includes(receipt.status)&&!currentAttemptIds.has(receipt.id))continue;
  if(loaded&&receipt.status==='SENT'&&!nativeTurns.has(turn))continue;
  const message:TimelineMessage={id:`receipt:${receipt.id}`,turnId:turn,role:'user',text:receipt.text,receipt};
  times.set(turn,Math.min(times.get(turn)??Infinity,receipt.createdAt));
  // A native page owns item order. Unreflected sends follow its known items;
  // with no native page, the sent prompt starts its resulting turn.
  if(receipt.status==='SENT'&&receipt.mode!=='STEER')group.unshift(message);else group.push(message);
  groups.set(turn,group);
 }
 const known=[...nativeTurns];
 const extra=[...groups.keys()].filter(turn=>!nativeTurns.has(turn)).sort((a,b)=>(times.get(a)??Infinity)-(times.get(b)??Infinity));
 // The tapped older notification remains before recent native turns. Other
 // locally observed new turns follow the native window until it refreshes.
 const before=extra.filter(turn=>sourceIsOlder&&turn===source.turnId&&known.length>0);
 const order=[...before,...known,...extra.filter(turn=>!before.includes(turn))];
 return order.flatMap(turn=>groups.get(turn)??[]);
}
