import type{CodexWatch}from'./CodexNotifications';
/** Native public message time owns recency; polling and status never reorder chats. */
export function useWatchVisitOrder(rows:CodexWatch[],_active:boolean){
 const at=(row:CodexWatch)=>typeof row.lastMessageAt==='number'&&Number.isFinite(row.lastMessageAt)&&row.lastMessageAt>0?row.lastMessageAt:0;
 return [...rows].sort((a,b)=>at(b)-at(a)||(a.threadId<b.threadId?-1:a.threadId>b.threadId?1:0));
}
