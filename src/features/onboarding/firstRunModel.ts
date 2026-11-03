import type {SharedCodexStatus} from '../workbench/SharedCodexConnection';
import type {HostedStatus} from '../workbench/HostedConnection';
export type SetupAgent='CODEX'|'CLAUDE_CODE';
export type SetupStep='CHOOSE'|'CODEX_PREPARE'|'CODEX_OPEN'|'CLAUDE_INSTALL'|'CLAUDE_LOAD'|'REMOTE'|'PHONE_CODE'|'PHONE_PAIR'|'PHONE_HOME'|'CLAUDE_GRANT'|'CLAUDE_MCP'|'CLAUDE_CONNECT'|'CLAUDE_AUTH'|'CLAUDE_VERIFY'|'CODEX_WATCH'|'CLAUDE_WATCH'|'PHONE_VERIFY'|'CLAUDE_USAGE'|'DONE';
export type SetupSnapshot={shared?:SharedCodexStatus;claudeLoaded?:boolean;mcpUrl?:string|null;hasGrant?:boolean;hosted?:HostedStatus;origin?:string|null;publicReady?:boolean;devices?:{id:string;label:string}[];existingUse?:boolean};
export type SetupPreference={version:1;disposition:'PENDING'|'SKIPPED'|'COMPLETED'|'EXISTING';agents:SetupAgent[];step:SetupStep;needsCheck?:SetupStep};
export const SETUP_STORAGE_KEY='agbrio.desktop-first-run.v1';
export const OPEN_SETUP_EVENT='agbrio:open-first-run';
export const setupStorage:Pick<Storage,'getItem'|'setItem'>={getItem:key=>{try{return window.localStorage.getItem(key);}catch{return null;}},setItem:(key,value)=>{try{window.localStorage.setItem(key,value);}catch{/* A blocked preference store must not block the owner. */}}};
const STEPS:SetupStep[]=['CHOOSE','CODEX_PREPARE','CODEX_OPEN','CLAUDE_INSTALL','CLAUDE_LOAD','REMOTE','PHONE_CODE','PHONE_PAIR','PHONE_HOME','CLAUDE_GRANT','CLAUDE_MCP','CLAUDE_CONNECT','CLAUDE_AUTH','CLAUDE_VERIFY','CODEX_WATCH','CLAUDE_WATCH','PHONE_VERIFY','CLAUDE_USAGE','DONE'];
export function readSetupPreference(storage:Pick<Storage,'getItem'>):SetupPreference|null{
 try{const value=JSON.parse(storage.getItem(SETUP_STORAGE_KEY)??'null');if(value?.version!==1||!['PENDING','SKIPPED','COMPLETED','EXISTING'].includes(value.disposition)||!STEPS.includes(value.step)||!Array.isArray(value.agents)||value.agents.some((a:unknown)=>a!=='CODEX'&&a!=='CLAUDE_CODE'))return null;return value;}catch{return null;}
}
export function writeSetupPreference(storage:Pick<Storage,'setItem'>,value:SetupPreference){try{storage.setItem(SETUP_STORAGE_KEY,JSON.stringify(value));}catch{/* Storage failure must never block setup or claim a connection. */}}
export function setupPlan(agents:SetupAgent[]):SetupStep[]{return ['CHOOSE',...(agents.includes('CODEX')?['CODEX_PREPARE','CODEX_OPEN'] as SetupStep[]:[]),...(agents.includes('CLAUDE_CODE')?['CLAUDE_INSTALL','CLAUDE_LOAD'] as SetupStep[]:[]),'REMOTE','PHONE_CODE','PHONE_PAIR','PHONE_HOME',...(agents.includes('CLAUDE_CODE')?['CLAUDE_GRANT','CLAUDE_MCP','CLAUDE_CONNECT','CLAUDE_AUTH','CLAUDE_VERIFY'] as SetupStep[]:[]),...(agents.includes('CODEX')?['CODEX_WATCH'] as SetupStep[]:[]),...(agents.includes('CLAUDE_CODE')?['CLAUDE_WATCH'] as SetupStep[]:[]),'PHONE_VERIFY',...(agents.includes('CLAUDE_CODE')?['CLAUDE_USAGE'] as SetupStep[]:[]),'DONE'];}
export function hostedRenewalRequired(s:SetupSnapshot){const h=s.hosted;return !!h?.origin&&h.origin.replace(/\/$/,'')===s.origin?.replace(/\/$/,'')&&(h.state==='EXPIRED'||h.state==='REVOKED'||!!h.expiresAt&&h.expiresAt<=(h.serverNow??Date.now()));}
export function remoteReady(s:SetupSnapshot){return !!s.origin&&s.publicReady===true&&!hostedRenewalRequired(s);}
/** Progress is presentation only. Only these observed facts allow a step to be skipped. */
export function observedStepComplete(step:SetupStep,s:SetupSnapshot){switch(step){case 'CODEX_PREPARE':return s.shared?.ready===true;case 'CODEX_OPEN':return s.shared?.ready===true&&s.shared.desktopConnected===true;case 'CLAUDE_INSTALL':case 'CLAUDE_LOAD':return s.claudeLoaded===true;case 'REMOTE':return remoteReady(s);case 'PHONE_CODE':case 'PHONE_PAIR':return (s.devices?.length??0)>0;case 'CLAUDE_GRANT':return s.hasGrant===true;default:return false;}}
export function nextSetupStep(plan:SetupStep[],after:SetupStep,s:SetupSnapshot):SetupStep{let index=plan.indexOf(after)+1;while(index<plan.length-1&&observedStepComplete(plan[index],s))index++;return plan[index]??'DONE';}
