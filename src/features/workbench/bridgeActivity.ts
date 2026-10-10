import {invoke} from "@tauri-apps/api/core";
import {request} from "../../mobile/api";
import type {BridgeRole,RoleActivity,AssistantProcessed} from "./RoleBridgePanel";
import {readModels, type ReadModelCache} from "./readModelCache";
export type BridgeActivity={assistantProcessed?:AssistantProcessed|null;workstreamId:string;bindingRevision:number;unreadCount?:number;latestRole?:BridgeRole|null;sides:RoleActivity[]};
export type BridgeActivityApi=((workstreamIds:string[])=>Promise<BridgeActivity[]>) & {
 cached?:(ids:string[])=>BridgeActivity[]|undefined;
 observedAt?:(ids:string[])=>number;
 subscribe?:(ids:string[],listener:()=>void)=>()=>void;
};
export function cachedBridgeActivity(loader:BridgeActivityApi, cache:ReadModelCache=readModels):BridgeActivityApi{
 const key=(ids:string[])=>`bridge-activity:${JSON.stringify([...new Set(ids)].sort())}`;
 return Object.assign((ids:string[])=>cache.read(key(ids),()=>loader(ids),5000),{
  cached:(ids:string[])=>cache.peek<BridgeActivity[]>(key(ids)),
  observedAt:(ids:string[])=>cache.observedAt(key(ids)),
  subscribe:(ids:string[],listener:()=>void)=>cache.subscribe(key(ids),listener),
 });
}
export const desktopBridgeActivity=cachedBridgeActivity(workstreamIds=>invoke("bridge_directory_activity",{workstreamIds}));
export const webBridgeActivity=cachedBridgeActivity(workstreamIds=>request("/bridge-activity",{method:"POST",body:JSON.stringify({workstreamIds})}));
export type ActivityPhase="GOAL_ACTIVE"|"THINKING"|"RUNNING"|"ACTION_REQUIRED"|"COMPLETE"|"RESULT_PENDING"|"EMPTY"|"PAUSED"|"LIMITED"|"FAILED"|"INTERRUPTED"|"UNCONFIRMED";
export function activityPhase(row:BridgeActivity|undefined,role:BridgeRole,age:number):ActivityPhase{
 const side=row?.sides.find(s=>s.role===role);if(!side||!side.checkedAt||age>=15000)return "UNCONFIRMED";
 if(side.state==="RUNNING"&&side.turnActive===false&&side.goalStatus==="active")return "GOAL_ACTIVE";
 return ["THINKING","RUNNING","ACTION_REQUIRED","COMPLETE","RESULT_PENDING","EMPTY","PAUSED","LIMITED","FAILED","INTERRUPTED"].includes(side.state)?side.state as ActivityPhase:"UNCONFIRMED";
}

/** The normal Bridge has one active side. Do not make users compare two panels;
 * preserve a truthful exception when two sides really run concurrently. */
export function focusedBridgeActivity(row:BridgeActivity,age:number){
 const sides=row.sides.map(side=>({...side,phase:activityPhase(row,side.role,age)}));
 const running=sides.filter(s=>["RUNNING","THINKING"].includes(s.phase));
 if(running.length>1)return {side:running[0],concurrent:true};
 const side=running[0]??sides.find(s=>["ACTION_REQUIRED","PAUSED","LIMITED","FAILED","INTERRUPTED"].includes(s.phase))??sides.find(s=>["GOAL_ACTIVE","RESULT_PENDING"].includes(s.phase))??sides.find(s=>s.role===row.latestRole)??sides.find(s=>s.phase==="COMPLETE");
 return side?{side,concurrent:false}:null;
}

export function bridgeReviewAttention(row:BridgeActivity|undefined,age:number){
 if(!row)return 0;const focus=focusedBridgeActivity(row,age);if(!focus)return 0;
 if(["RUNNING","THINKING","GOAL_ACTIVE","RESULT_PENDING"].includes(focus.side.phase))return 0;
 if(["ACTION_REQUIRED","FAILED","LIMITED"].includes(focus.side.phase))return Math.max(1,row.unreadCount??0);
 return ["COMPLETE","PAUSED","INTERRUPTED"].includes(focus.side.phase)?row.unreadCount??0:0;
}

/** A durable SENT attribution bridges the gap until that exact recipient turn is
 * observed. It never suppresses a different source or a real pending decision. */
export function assistantProcessedVisible(row:BridgeActivity,now:number,age:number){
 const mark=row.assistantProcessed;if(!mark||now<mark.sentAt||now>=mark.expiresAt)return false;
 if(row.sides.some(side=>['ACTION_REQUIRED','FAILED','LIMITED'].includes(side.state)))return false;
 const target=row.sides.find(side=>side.endpointId===mark.destinationEndpointId);
 return !(target&&age<15000&&target.checkedAt>=mark.sentAt&&mark.destinationTurnId&&target.turnId===mark.destinationTurnId&&['RUNNING','THINKING','RESULT_PENDING','COMPLETE','PAUSED','FAILED','INTERRUPTED'].includes(target.state));
}
