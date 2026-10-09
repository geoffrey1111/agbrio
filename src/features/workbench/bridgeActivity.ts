import {invoke} from "@tauri-apps/api/core";
import {request} from "../../mobile/api";
import type {BridgeRole,RoleActivity} from "./RoleBridgePanel";
export type BridgeActivity={workstreamId:string;bindingRevision:number;unreadCount?:number;latestRole?:BridgeRole|null;sides:RoleActivity[]};
export type BridgeActivityApi=(workstreamIds:string[])=>Promise<BridgeActivity[]>;
export const desktopBridgeActivity:BridgeActivityApi=workstreamIds=>invoke("bridge_directory_activity",{workstreamIds});
export const webBridgeActivity:BridgeActivityApi=workstreamIds=>request("/bridge-activity",{method:"POST",body:JSON.stringify({workstreamIds})});
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
