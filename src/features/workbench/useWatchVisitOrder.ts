import type{CodexWatch}from'./CodexNotifications';
/** Preserve activity and paused groups; native public message time orders each group. */
function group(row:CodexWatch){
 if(!row.enabled)return 3;
 if(row.errorCode)return 2;
 if(row.snapshot.state==='ACTION_REQUIRED'||row.snapshot.state==='FAILED'||row.taskTree?.attention)return 1;
 if(row.snapshot.state==='RUNNING'||row.taskTree?.running)return 0;
 return 2;
}
export function useWatchVisitOrder(rows:CodexWatch[],_active:boolean){
 const at=(row:CodexWatch)=>typeof row.lastMessageAt==='number'&&Number.isFinite(row.lastMessageAt)&&row.lastMessageAt>0?row.lastMessageAt:0;
 return [...rows].sort((a,b)=>group(a)-group(b)||at(b)-at(a)||(a.threadId<b.threadId?-1:a.threadId>b.threadId?1:0));
}
