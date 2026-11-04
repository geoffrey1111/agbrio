/** Disposable visual fixture; no real provider, grant, subscription or task. */
import {createRoot} from 'react-dom/client';
import {ManagementZones} from '../../src/features/workbench/ManagementZones';
import type {ManagedConversation,ManagementSnapshot,ManagementApi} from '../../src/features/workbench/managementZoneApi';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/conversation-v6.css';
import '../../src/styles/agbrio-v7.css';
const row=(id:string,label:string,provider:ManagedConversation['provider'],state='RUNNING'):ManagedConversation=>({threadId:id,label,provider,cwd:'D:/项目/铸阵',generation:1,state,checkedAt:Date.now(),available:true,watchEnabled:true});
const manager=row('claude_code:manager-a','游戏开发管理','CLAUDE_CODE'),backup=row('manager-b','工程管理','CODEX'),art=row('art','角色立绘与武器','CODEX','ACTION_REQUIRED'),engineering=row('engineering','战斗规则与数值','CODEX'),ui=row('pi:design','界面设计与验收','PI','RESULT_READY');
const snapshot:ManagementSnapshot={zones:[{id:'game',name:'游戏开发',managerId:manager.threadId,manager,revision:1,updatedAt:Date.now(),members:[art,engineering,ui].map(conversation=>({threadId:conversation.threadId,generation:1,conversation}))},{id:'release',name:'发布维护',managerId:backup.threadId,manager:backup,revision:1,updatedAt:Date.now(),members:[]}],candidates:[manager,backup,art,engineering,ui],legacyAssignments:[]};
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
createRoot(document.getElementById('root')!).render(<main style={{minHeight:'100dvh',background:'var(--v7-bg)',fontFamily:'var(--r2-font, sans-serif)'}}><p style={{margin:0,padding:'8px 20px',fontSize:12,color:'var(--v7-muted)'}}>隔离界面验收 · 演示分工，不连接真实对话</p><ManagementZones api={api} onOpenWatch={()=>{}}/></main>);
