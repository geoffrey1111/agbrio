/** Disposable visual fixture; no real provider, grant, subscription or task. */
import {createRoot} from 'react-dom/client';
import {ManagementZones} from '../../src/features/workbench/ManagementZones';
import type {NotificationApi} from '../../src/features/workbench/CodexNotifications';
import type {ManagedConversation,ManagementSnapshot,ManagementApi} from '../../src/features/workbench/managementZoneApi';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/conversation-v6.css';
import '../../src/styles/agbrio-v7.css';
const row=(id:string,label:string,provider:ManagedConversation['provider'],state='RUNNING'):ManagedConversation=>({threadId:id,label,provider,cwd:'D:/项目/铸阵',generation:1,state,checkedAt:Date.now(),available:true,watchEnabled:true});
const manager=row('claude_code:manager-a','游戏开发管理（11-04 交接接管）','CLAUDE_CODE'),backup=row('manager-b','工程管理','CODEX'),mac=row('claude_code:mac','Mac 适配方案','CLAUDE_CODE','RESULT_READY');
const work:[string,string,ManagedConversation['provider'],string][]=[['art','接续角色建模与ARP制作','CODEX','RESULT_READY'],['design','接手角色设计与立绘制作','CODEX','ACTION_REQUIRED'],['card','制作随从卡插画小样','CODEX','RESULT_READY'],['ui','UI 主题手绘小样（v1.1）','CODEX','RUNNING'],['scene','战场场景生图（S1 林间废墟试点）','CODEX','RESULT_READY'],['tcg','解包并分析 Origins TCG UI','CODEX','IDLE'],['board','美术看板前端维护','CODEX','FAILED'],['eng','Codex 工程执行（管理执行单）','CODEX','RUNNING'],['mixamo','Mixamo 检索能力试验（Pawn101 攻击）','CODEX','RUNNING'],['batch','批量建模（可建模 48 位新体）','CODEX','RUNNING'],['pi:ui','界面设计与验收','PI','RESULT_READY']];
const big=work.map(([id,label,p,state])=>row(id,label,p,state));
const count=Number(new URLSearchParams(location.search).get('members')??10);
const spare=[row('spare-1','首领切入插画','CODEX','RESULT_READY'),row('spare-2','制作正式 UI 资源关键样张','CODEX','IDLE')];
const snapshot:ManagementSnapshot=new URLSearchParams(location.search).get('empty')?{zones:[],candidates:[manager,backup,mac,...big],legacyAssignments:[{managerId:manager.threadId,threadId:'art'},{managerId:manager.threadId,threadId:'card'}]}:{zones:[{id:'game',name:'游戏开发管理',managerId:manager.threadId,manager,revision:1,updatedAt:Date.now(),members:big.slice(0,count).map(conversation=>({threadId:conversation.threadId,generation:1,conversation}))},{id:'mac',name:'Mac 适配方案',managerId:mac.threadId,manager:mac,revision:1,updatedAt:Date.now(),members:big.slice(count).map(conversation=>({threadId:conversation.threadId,generation:1,conversation}))},{id:'release',name:'发布维护',managerId:backup.threadId,manager:backup,revision:1,updatedAt:Date.now(),members:[]}],candidates:[manager,backup,mac,...big,...spare],legacyAssignments:[]};
const receipts=new Map<string,unknown>();
const api:ManagementApi={read:async()=>structuredClone(snapshot),change:async input=>{
 if(receipts.has(input.requestId))return receipts.get(input.requestId);
 if(input.action==='CREATE'){const id=crypto.randomUUID();snapshot.zones.push({id,name:input.name!,managerId:input.managerId!,manager:snapshot.candidates.find(c=>c.threadId===input.managerId)!,revision:1,updatedAt:Date.now(),members:[]});}
 const target=snapshot.zones.find(z=>z.id===input.zoneId);
 if(target){
  if(input.action==='APPOINT'){target.managerId=input.managerId!;target.manager=snapshot.candidates.find(c=>c.threadId===input.managerId)!;}
  if(input.action==='RENAME')target.name=input.name!;
  if(input.action==='MOVE'){const old=snapshot.zones.find(z=>z.id===input.fromZoneId)!;const member=old.members.find(m=>m.threadId===input.threadId)!;old.members=old.members.filter(m=>m!==member);old.revision++;target.members.push(member);}
  if(input.action==='REMOVE')target.members=target.members.filter(m=>m.threadId!==input.threadId);
  if(input.action==='ADD'){const conversation=snapshot.candidates.find(c=>c.threadId===input.threadId)!;target.members.push({threadId:conversation.threadId,generation:conversation.generation,conversation});}
  if(input.action==='DELETE')snapshot.zones=snapshot.zones.filter(z=>z!==target);else target.revision++;
 }
 const receipt={requestId:input.requestId};receipts.set(input.requestId,receipt);return receipt;
}};
createRoot(document.getElementById('root')!).render(<main style={{minHeight:'100dvh',background:'var(--v7-bg)',fontFamily:'var(--r2-font, sans-serif)'}}><p style={{margin:0,padding:'8px 20px',fontSize:12,color:'var(--v7-muted)'}}>隔离界面验收 · 演示分工，不连接真实对话</p><ManagementZones api={api} watchApi={{enable:async()=>{},watches:async()=>[]} as unknown as NotificationApi} onOpenWatch={()=>{}}/></main>);
