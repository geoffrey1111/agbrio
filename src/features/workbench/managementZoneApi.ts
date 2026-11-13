import {invoke,isTauri} from '@tauri-apps/api/core';
import {liveRequest} from '../../mobile/api';
export type ManagedConversation={threadId:string;localSessionId?:string;lastActivityAt?:number;label:string;provider:'CLAUDE_CODE'|'CODEX'|'PI';cwd:string;generation:number;state:string;checkedAt:number;available:boolean;watchEnabled:boolean};
export type ManagementZone={id:string;name:string;managerId:string|null;manager:ManagedConversation|null;revision:number;updatedAt:number;members:{threadId:string;generation:number;conversation:ManagedConversation|null}[]};
export type ManagementSnapshot={zones:ManagementZone[];candidates:ManagedConversation[];legacyAssignments:{managerId:string;threadId:string}[]};
export type ManagementChange={requestId:string;action:'CREATE'|'ADOPT'|'APPOINT'|'ADD'|'MOVE'|'REMOVE'|'RENAME'|'DELETE';zoneId?:string;revision?:number;name?:string;managerId?:string;threadId?:string;generation?:number;fromRevision?:number;fromZoneId?:string;threadIds?:string[]};
export type ManagementApi={read:()=>Promise<ManagementSnapshot>;change:(input:ManagementChange)=>Promise<unknown>};
export const managementZoneApi:ManagementApi={
 read:()=>isTauri()?invoke('management_zones_read'):liveRequest('/management-zones'),
 change:input=>isTauri()?invoke('management_zones_change',{input}):liveRequest('/management-zones',{method:'POST',body:JSON.stringify(input)}),
};
