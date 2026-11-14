import type {ManagedConversation,ManagementSnapshot,ManagementApi} from '../../src/features/workbench/managementZoneApi';
const row=(id:string,label:string,provider:ManagedConversation['provider'],state='RUNNING'):ManagedConversation=>({threadId:id,label,provider,cwd:'D:/项目/铸阵',generation:1,state,checkedAt:Date.now(),available:true,watchEnabled:true});
const manager=row('claude_code:manager-a','游戏开发管理（11-04 交接接管）','CLAUDE_CODE'),backup=row('manager-b','工程管理','CODEX'),mac=row('claude_code:mac','Mac 适配方案','CLAUDE_CODE','RESULT_READY');
const work:[string,string,ManagedConversation['provider'],string][]=[['art','接续角色建模与ARP制作','CODEX','RESULT_READY'],['design','接手角色设计与立绘制作','CODEX','ACTION_REQUIRED'],['card','制作随从卡插画小样','CODEX','RESULT_READY'],['ui','UI 主题手绘小样（v1.1）','CODEX','RUNNING'],['scene','战场场景生图（S1 林间废墟试点）','CODEX','RESULT_READY'],['tcg','解包并分析 Origins TCG UI','CODEX','IDLE'],['board','美术看板前端维护','CODEX','FAILED'],['eng','Codex 工程执行（管理执行单）','CODEX','RUNNING'],['mixamo','Mixamo 检索能力试验（Pawn101 攻击）','CODEX','RUNNING'],['batch','批量建模（可建模 48 位新体）','CODEX','RUNNING'],['pi:ui','界面设计与验收','PI','RESULT_READY']];
const big=work.map(([id,label,p,state])=>row(id,label,p,state));
const inactivityFixture=new URLSearchParams(location.search).has('inactivity');
if(inactivityFixture)for(const conversation of big.filter(row=>['art','card','design','ui'].includes(row.threadId)))conversation.lastActivityAt=Date.now()-4*24*60*60*1000;
const count=Number(new URLSearchParams(location.search).get('members')??10);
const spare=[row('spare-1','首领切入插画','CODEX','RESULT_READY'),row('spare-2','制作正式 UI 资源关键样张','CODEX','IDLE')];
const snapshot:ManagementSnapshot=new URLSearchParams(location.search).get('empty')?{zones:[],candidates:[manager,backup,mac,...big],legacyAssignments:[{managerId:manager.threadId,threadId:'art'},{managerId:manager.threadId,threadId:'card'}]}:{zones:[{id:'game',name:'游戏开发管理',managerId:manager.threadId,manager,revision:1,updatedAt:Date.now(),members:big.slice(0,count).map(conversation=>({threadId:conversation.threadId,generation:1,conversation}))},{id:'mac',name:'Mac 适配方案',managerId:mac.threadId,manager:mac,revision:1,updatedAt:Date.now(),members:big.slice(count).map(conversation=>({threadId:conversation.threadId,generation:1,conversation}))},{id:'release',name:'发布维护',managerId:backup.threadId,manager:backup,revision:1,updatedAt:Date.now(),members:[]}],candidates:[manager,backup,mac,...big,...spare],legacyAssignments:[]};
if(new URLSearchParams(location.search).has('legacy'))snapshot.legacyAssignments=spare.map(c=>({managerId:manager.threadId,threadId:c.threadId}));
let first=true;
const lag=()=>new Promise(resolve=>setTimeout(resolve,320));
const receipts=new Map<string,unknown>();
export const demoManagement:ManagementApi={read:async()=>{await lag();return structuredClone(snapshot);},change:async input=>{
 await lag();
 if(receipts.has(input.requestId))return receipts.get(input.requestId);
 if(input.action==='CREATE'){const id=crypto.randomUUID();snapshot.zones.push({id,name:input.name!,managerId:input.managerId!,manager:snapshot.candidates.find(c=>c.threadId===input.managerId)!,revision:1,updatedAt:Date.now(),members:[]});}
 if(input.action==='ADOPT'){
  let area=snapshot.zones.find(z=>z.id===input.zoneId);
  if(!area){area={id:crypto.randomUUID(),name:input.name!,managerId:input.managerId!,manager:snapshot.candidates.find(c=>c.threadId===input.managerId)!,revision:1,updatedAt:Date.now(),members:[]};snapshot.zones.push(area);}
  for(const id of input.threadIds??[]){const conversation=snapshot.candidates.find(c=>c.threadId===id)!;area.members.push({threadId:id,generation:conversation.generation,conversation});}
  snapshot.legacyAssignments=snapshot.legacyAssignments.filter(l=>!input.threadIds?.includes(l.threadId));
 }
 const target=snapshot.zones.find(z=>z.id===input.zoneId);
 if(target){
  if(input.action==='APPOINT'){target.managerId=input.managerId!;target.manager=snapshot.candidates.find(c=>c.threadId===input.managerId)!;}
  if(input.action==='RENAME')target.name=input.name!;
  if(input.action==='MOVE'){const old=snapshot.zones.find(z=>z.id===input.fromZoneId)!;const member=old.members.find(m=>m.threadId===input.threadId)!;old.members=old.members.filter(m=>m!==member);old.revision++;target.members.push(member);}
  if(input.action==='REMOVE')target.members=target.members.filter(m=>m.threadId!==input.threadId);
  if(input.action==='ADD'){const conversation=snapshot.candidates.find(c=>c.threadId===input.threadId)!;target.members.push({threadId:conversation.threadId,generation:conversation.generation,conversation});}
  if(input.action==='DELETE')snapshot.zones=snapshot.zones.filter(z=>z!==target);else target.revision++;
 }
 const receipt={requestId:input.requestId,zoneId:target?.id??snapshot.zones.at(-1)?.id};receipts.set(input.requestId,receipt);
 if(first&&new URLSearchParams(location.search).has('uncertain')){first=false;throw Error('NETWORK');}
 return receipt;
}};

export const demoConversation=(id:string)=>snapshot.candidates.find(c=>c.threadId===id);
