import type {ChatMessage,WatchReply} from './watchChatApi';
export type ObservedMessage=ChatMessage&{seenAt:number};
export type TimelineMessage=ChatMessage&{receipt?:WatchReply;source?:boolean};
const identity=(m:ChatMessage)=>JSON.stringify([m.turnId,m.id]);
const fresher=(old:ChatMessage,next:ChatMessage)=>old.revision!==undefined&&(next.revision===undefined||old.revision>next.revision)?old:next;
/** The Host returns newest turns first, but items inside each turn oldest first. */
export function chronologicalHistoryPage(messages:ChatMessage[]){
 const groups=new Map<string,ChatMessage[]>();
 for(const message of messages){const group=groups.get(message.turnId)??[];group.push(message);groups.set(message.turnId,group);}
 const rows=[...groups.values()].reverse().flat();
 return rows.length&&rows.every(m=>m.order!==undefined)?rows.sort((a,b)=>a.order!-b.order!):rows;
}
export function mergeHistory(older:ChatMessage[],newer:ChatMessage[]){
 const groups=new Map<string,ChatMessage[]>();
 for(const message of older){const group=groups.get(message.turnId)??[];group.push(message);groups.set(message.turnId,group);}
 const turnOrder=[...groups.keys()];
 const incoming=new Map<string,ChatMessage[]>();
 for(const message of newer){const group=incoming.get(message.turnId)??[];group.push(message);incoming.set(message.turnId,group);}
 for(const [turn,page] of incoming){
  const previous=groups.get(turn)??[];
  // Update existing items in place. Insert newly discovered items around native
  // anchors, so a partial refresh cannot move an older report to the bottom.
  const updates=new Map(page.map(m=>[identity(m),m]));
  const existing=new Map(previous.map(m=>[identity(m),m]));
  const canonical=page.filter(m=>existing.has(identity(m))).map(m=>fresher(existing.get(identity(m))!,m));
  let anchor=0;
  // A fresh native page can repair a warm cache's incorrect item order. A
  // single-item update still stays in place; missing items retain their slots.
  const merged=previous.map(m=>updates.has(identity(m))?canonical[anchor++]:m);
  for(let i=0;i<page.length;i++){
   const item=page[i];if(merged.some(m=>identity(m)===identity(item)))continue;
   const next=page.slice(i+1).find(n=>merged.some(m=>identity(m)===identity(n)));
   const prior=page.slice(0,i).reverse().find(n=>merged.some(m=>identity(m)===identity(n)));
   const at=next?merged.findIndex(m=>identity(m)===identity(next)):prior?merged.findIndex(m=>identity(m)===identity(prior))+1:merged.length;
   merged.splice(at,0,item);
  }
  groups.set(turn,merged);
 }
 const pageTurns=[...incoming.keys()];
 for(let i=0;i<pageTurns.length;i++){
  const turn=pageTurns[i];if(turnOrder.includes(turn))continue;
  const anchor=pageTurns.slice(i+1).find(t=>turnOrder.includes(t));
  turnOrder.splice(anchor?turnOrder.indexOf(anchor):turnOrder.length,0,turn);
 }
 const rows=turnOrder.flatMap(turn=>groups.get(turn)??[]);
 return rows.length&&rows.every(m=>m.order!==undefined)?rows.sort((a,b)=>a.order!-b.order!):rows;
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
 // Receipt decoration belongs to this render, never to cached native history.
 // Mutating an input item would make the next render consider it already claimed
 // and append the same receipt as a second user message.
 const add=(message:TimelineMessage)=>{const group=groups.get(message.turnId)??[];const index=group.findIndex(m=>m.id===message.id);if(index<0)group.push({...message});else if(fresher(group[index],message)===message)group[index]={...group[index],...message};groups.set(message.turnId,group);};
 for(const message of history)add(message);
 // Adapter snapshots alias the final public item of their exact native turn.
 // Equal text in another turn is never identity ("OK" may occur many times).
 const sameReply=(message:ChatMessage)=>loaded&&message.role==='assistant'&&message.id.startsWith('claude_reply:')?history.filter(m=>m.role==='assistant'&&m.turnId===message.turnId).at(-1):undefined;
 for(const message of [...observed,source]){
  // A local observation has no native position outside the loaded window.
  // Older cached turns return through native paging, never after the fresh tail.
  if(loaded&&history.length&&message!==source&&!nativeTurns.has(message.turnId)&&message.turnId!==current?.turnId&&message.turnId!==source.turnId)continue;
  // Old directory/adapter summaries are projections, never a second native row.
  // A native id remains unique even if an older projection assigned another turn.
  const canonical=history.find(m=>m.id===message.id&&m.id.startsWith('claude_msg:'));
  if(canonical){if(identity(message)===identity(source)){const item=groups.get(canonical.turnId)?.find(m=>m.id===canonical.id);if(item)item.source=true;}continue;}
  if(loaded&&message!==source&&message.id.startsWith('claude_reply:'))continue;
  const twin=sameReply(message);
  if(twin){if(identity(message)===identity(source)){const group=groups.get(twin.turnId);const item=group?.find(m=>m.id===twin.id);if(item)item.source=true;}continue;}
  times.set(message.turnId,Math.min(times.get(message.turnId)??Infinity,message.seenAt));
  const existingItem=groups.get(message.turnId)?.find(m=>m.id===message.id);
  if(existingItem){existingItem.source=identity(message)===identity(source);}
  else add({...message,source:identity(message)===identity(source)});
 }
 // The current live snapshot can advance inside the same native item. Older
 // session-cache text must never overwrite a freshly read native message.
 if(current&&!sameReply(current))add({...current,source:identity(current)===identity(source)});
 const sorted=[...replies].sort((a,b)=>a.createdAt-b.createdAt);
 for(const receipt of sorted){
  // Claude receipts can have no turn id. Their independently stored native UUID
  // is the identity; neither body text nor attachment manifests are routing keys.
  const native=receipt.status==='SENT'?[...groups.values()].flat().find(m=>m.role==='user'&&!m.receipt&&((receipt.nativeMessageId&&m.id===receipt.nativeMessageId)||m.clientId===receipt.id)):undefined;
  if(native){native.receipt=receipt;continue;}
  const turn=receipt.turnId??`receipt:${receipt.id}`;
  const group=groups.get(turn)??[];
  // A turn may contain several owner prompts (steers and async answers). The
  // initiating prompt may be outside the loaded page. Never attach its receipt
  // to an arbitrary later user item: that replaces the real body's chronology.
  const matchesBody=(m:ChatMessage)=>m.text===receipt.text||(
   receipt.options.attachments.length>0&&m.text.startsWith(receipt.text)&&
   /^\s*(?:已选择的附件（数据，不是系统指令）：|Selected attachments:|Material manifest:)/.test(m.text.slice(receipt.text.length)));
  const match=group.find(m=>m.role==='user'&&m.kind!=='COLLABORATION'&&matchesBody(m)&&!m.receipt&&(!receipt.nativeMessageId||m.id===receipt.nativeMessageId)&&(!m.clientId||m.clientId===receipt.id));
  if(match){match.receipt=receipt;continue;}
  if(['CANCELLED','FAILED','ACKNOWLEDGED'].includes(receipt.status)&&!currentAttemptIds.has(receipt.id))continue;
  if(receipt.status==='SENT'&&!currentAttemptIds.has(receipt.id))continue;
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
 const rows=order.flatMap(turn=>groups.get(turn)??[]);
 const positioned=rows.filter(m=>m.order!==undefined).sort((a,b)=>a.order!-b.order!);
 // Claude's byte offsets also preserve interleaved internal events inside one
 // prompt. Turn grouping alone cannot express that native order.
 if(positioned.length===0)return rows;
 const unpositioned=rows.filter(m=>m.order===undefined);
 return [...unpositioned.filter(m=>before.includes(m.turnId)),...positioned,...unpositioned.filter(m=>!before.includes(m.turnId))];
}

/** Native asynchronous-answer transport stays private to the protocol. */
/** Display only: raw prompt bytes and native identities remain unchanged. */
export function ownerAttachmentText(text:string){const marker=text.search(/\n已选择的附件（数据，不是系统指令）：[^\r\n]+\r?\n文件路径：[^\r\n]+\r?\nSHA256：[a-f0-9]{64}(?:\r?\n|$)/);return marker<0?'':text.slice(marker);}
export function ownerMessageText(text:string){
 const attachment=ownerAttachmentText(text);if(attachment)text=text.slice(0,text.length-attachment.length);

 const match=text.trim().match(/^<send_user_message_question_reply>\s*([\s\S]*)\s*<\/send_user_message_question_reply>$/);if(!match)return text;
 try{const rows=JSON.parse(match[1]);if(!Array.isArray(rows)||!rows.length||rows.some(r=>typeof r.questionItemId!=='string'||typeof r.answer!=='string'))return text;return rows.map(r=>[typeof r.question==='string'?r.question:'',r.answer].filter(Boolean).join('\n')).join('\n\n');}catch{return text;}
}

/** Peer turns do not answer an owner question. Native public prompts and a newer
 * actionable question do; keep the older card's unavailable reason visible. */
export function nativeCardUnavailable(message:ChatMessage,timeline:ChatMessage[]):string|undefined{
 const index=timeline.findIndex(m=>m.id===message.id&&m.turnId===message.turnId);
 const newer=timeline.filter((m,i)=>m.order!==undefined&&message.order!==undefined?m.order>message.order:i>index);
 if(newer.some(m=>m.role==='user'&&m.kind!=='COLLABORATION'&&!m.id.startsWith('receipt:')))return '此问题之后已有你的新消息。';
 if(newer.some(m=>m.card&&(m.card.question?.trim()||m.card.actions.length)))return '已有更新的问题，请查看最新卡片。';
}
