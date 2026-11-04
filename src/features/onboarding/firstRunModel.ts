import type {SharedCodexStatus} from '../workbench/SharedCodexConnection';
import type {HostedStatus} from '../workbench/HostedConnection';
export type SetupAgent='CODEX'|'CLAUDE_CODE';
/** One screen per user goal; background actions inside a goal run in place. */
export type SetupStep='CHOOSE'|'CODEX'|'PHONE'|'CLAUDE'|'WATCH'|'VERIFY'|'DONE';
export type SetupSnapshot={shared?:SharedCodexStatus;claudeLoaded?:boolean;mcpUrl?:string|null;hasGrant?:boolean;hosted?:HostedStatus;origin?:string|null;publicReady?:boolean;devices?:{id:string;label:string}[];existingUse?:boolean};
export type SetupPreference={version:1;disposition:'PENDING'|'SKIPPED'|'COMPLETED'|'EXISTING';agents:SetupAgent[];step:SetupStep;needsCheck?:SetupStep};
export const SETUP_STORAGE_KEY='agbrio.desktop-first-run.v1';
export const OPEN_SETUP_EVENT='agbrio:open-first-run';
export const setupStorage:Pick<Storage,'getItem'|'setItem'>={getItem:key=>{try{return window.localStorage.getItem(key);}catch{return null;}},setItem:(key,value)=>{try{window.localStorage.setItem(key,value);}catch{/* A blocked preference store must not block the owner. */}}};
const STEPS:SetupStep[]=['CHOOSE','CODEX','PHONE','CLAUDE','WATCH','VERIFY','DONE'];
/** Saved progress from the earlier per-action wizard resumes on the goal screen that now owns it. */
const LEGACY:Record<string,SetupStep>={CODEX_PREPARE:'CODEX',CODEX_OPEN:'CODEX',REMOTE:'PHONE',PHONE_CODE:'PHONE',PHONE_PAIR:'PHONE',PHONE_HOME:'PHONE',CLAUDE_INSTALL:'CLAUDE',CLAUDE_LOAD:'CLAUDE',CLAUDE_GRANT:'CLAUDE',CLAUDE_MCP:'CLAUDE',CLAUDE_CONNECT:'CLAUDE',CLAUDE_AUTH:'CLAUDE',CLAUDE_VERIFY:'CLAUDE',CODEX_WATCH:'WATCH',CLAUDE_WATCH:'WATCH',PHONE_VERIFY:'VERIFY',CLAUDE_USAGE:'DONE'};
const normalize=(step:unknown):SetupStep|null=>typeof step!=='string'?null:STEPS.includes(step as SetupStep)?step as SetupStep:LEGACY[step]??null;
export function readSetupPreference(storage:Pick<Storage,'getItem'>):SetupPreference|null{
 try{const value=JSON.parse(storage.getItem(SETUP_STORAGE_KEY)??'null');const step=normalize(value?.step);if(value?.version!==1||!['PENDING','SKIPPED','COMPLETED','EXISTING'].includes(value.disposition)||!step||!Array.isArray(value.agents)||value.agents.some((a:unknown)=>a!=='CODEX'&&a!=='CLAUDE_CODE'))return null;const needsCheck=normalize(value.needsCheck);return {version:1,disposition:value.disposition,agents:value.agents,step,...(needsCheck?{needsCheck}:{})};}catch{return null;}
}
export function writeSetupPreference(storage:Pick<Storage,'setItem'>,value:SetupPreference){try{storage.setItem(SETUP_STORAGE_KEY,JSON.stringify(value));}catch{/* Storage failure must never block setup or claim a connection. */}}
/** Phone/address comes before Claude because the MCP endpoint is the same public address. */
export function setupPlan(agents:SetupAgent[]):SetupStep[]{return ['CHOOSE',...(agents.includes('CODEX')?['CODEX'] as SetupStep[]:[]),'PHONE',...(agents.includes('CLAUDE_CODE')?['CLAUDE'] as SetupStep[]:[]),'WATCH','VERIFY','DONE'];}
export function hostedRenewalRequired(s:SetupSnapshot){const h=s.hosted;return !!h?.origin&&h.origin.replace(/\/$/,'')===s.origin?.replace(/\/$/,'')&&(h.state==='EXPIRED'||h.state==='REVOKED'||!!h.expiresAt&&h.expiresAt<=(h.serverNow??Date.now()));}
export function remoteReady(s:SetupSnapshot){return !!s.origin&&s.publicReady===true&&!hostedRenewalRequired(s);}
export const codexReady=(s:SetupSnapshot)=>s.shared?.ready===true&&s.shared.desktopConnected===true;
export const phoneReady=(s:SetupSnapshot)=>remoteReady(s)&&(s.devices?.length??0)>0;
/** Progress is presentation only. Only these observed facts allow a whole goal screen to be skipped. */
export function observedStepComplete(step:SetupStep,s:SetupSnapshot){switch(step){case 'CODEX':return codexReady(s);case 'PHONE':return phoneReady(s);default:return false;}}
export function nextSetupStep(plan:SetupStep[],after:SetupStep,s:SetupSnapshot):SetupStep{let index=plan.indexOf(after)+1;while(index<plan.length-1&&observedStepComplete(plan[index],s))index++;return plan[index]??'DONE';}
