import type {CodexWatch,WatchPending} from './CodexNotifications';
import type {BridgeActivity} from './bridgeActivity';
import type {WorkbenchItem} from './models';
import {failedAttentionKey} from './attentionDismissals';
import {claudeChildFailureKey,childNodeKey,type ClaudeNode} from './childTree';
import {piDelegations} from './piDelegations';
import {bridgeTone} from './ConversationHome';
import {messagePreview} from './messagePreview';
import {t,tc} from '../../i18n';
import {providerOwnerAttention} from './providerAttention';

/** To-do = only what needs the owner's hands. Cards are derived from live
 * native state, so answered, sent or re-run items disappear by themselves.
 * Reading a result never removes an unanswered question. */
export type TodoGroup='OWNER'|'ANSWER'|'SEND'|'FAILED';
export type TodoCard={key:string;group:TodoGroup;title:string;source:string;text:string;at:number;action:string;avatar:{id:string;kind:'bridge'|'codex'|'claude'|'pi'};detail?:string;next?:string;watch?:CodexWatch;childId?:string;parentLabel?:string;bridgeId?:string;dismissKey?:string};
type ActivityRows=Record<string,{row:BridgeActivity;receivedAt:number}>;

const kindOf=(id:string)=>id.startsWith('pi:')?'pi' as const:id.startsWith('claude_code:')?'claude' as const:'codex' as const;
const time=(v:number|null|undefined)=>typeof v==='number'&&Number.isFinite(v)&&v>0?v:0;
const pendingText=(p:WatchPending)=>p.prompt?.trim()||t(p.kind==='QUESTION'?'有一个问题等你回答':p.kind==='PERMISSION'?'有一项权限请求等你确认':'有一项操作等你批准');
const pendingAction=(p:WatchPending,pi:boolean)=>pi||!p.canRespond?t('打开对话'):p.kind==='QUESTION'?t('去回答'):t('去确认');

export function deriveTodoCards({watches,bridges,activity,activityClock,isDismissed}:{watches:CodexWatch[];bridges:WorkbenchItem[];activity:ActivityRows;activityClock:number;isDismissed:(id:string)=>boolean}):TodoCard[]{
 const cards=new Map<string,TodoCard>();
 const add=(c:TodoCard)=>{if(!cards.has(c.key))cards.set(c.key,c);};
 const watched=new Set(watches.map(w=>w.threadId));
 const delegation=piDelegations(watches);
 const parentOf=new Map<string,CodexWatch>();
 for(const [parent,children] of delegation.children){const p=watches.find(w=>w.threadId===parent);if(p)for(const c of children)parentOf.set(c.threadId,p);}
 for(const w of watches){
  const pi=w.threadId.startsWith('pi:');
  const ownerReason=pi?providerOwnerAttention(w.errorCode):null;
  // A classified upstream account problem: reason, the upstream wording as received, and the next step.
  if(ownerReason){const key=failedAttentionKey(w);const upstream=messagePreview(w.snapshot.text).slice(0,160);if(!isDismissed(key))add({key:JSON.stringify(['provider-owner',key]),group:'OWNER',title:w.label,source:parentOf.get(w.threadId)?t('{0} 的 Pi 子任务',parentOf.get(w.threadId)!.label):'Pi',text:t(ownerReason.reason),detail:upstream||undefined,next:t(ownerReason.next),at:time(w.lastMessageAt)||time(w.changedAt),action:t('打开对话'),avatar:{id:`w:${w.threadId}`,kind:'pi'},watch:w,parentLabel:parentOf.get(w.threadId)?.label,dismissKey:key});continue;}
  if((!w.enabled&&!pi)||(w.errorCode&&!(pi&&w.snapshot.state==='FAILED'&&w.errorCode.startsWith('PI_PROVIDER_'))))continue;
  const parent=parentOf.get(w.threadId);
  const source=parent?t('{0} 的 Pi 子任务',parent.label):w.threadId.startsWith('claude_code:')?'Claude Code':pi?'Pi':'Codex';
  const nodes=w.taskTree?.nodes??[];
  const nodeOf=(id:string|null)=>id?nodes.find(n=>n.sessionId===id):undefined;
  if(w.pending){
   // Transport T2: exact unanswered native requests (Codex, Claude parent/child, read-only Pi).
   for(const p of w.pending){
    const node=nodeOf(p.childId);
    if(node&&watched.has(childNodeKey(node.sessionId)))continue; // the child's own watch carries it
    add({key:JSON.stringify(['ask',w.threadId,p.childId,p.requestId,p.revision]),group:'ANSWER',title:node?.title??w.label,source:node?t('{0} 的子对话',w.label):source,text:pendingText(p),at:time(p.since)||time(w.lastMessageAt),action:pendingAction(p,pi),avatar:{id:`w:${w.threadId}`,kind:kindOf(w.threadId)},watch:w,childId:node?.sessionId,parentLabel:parent?.label});
   }
  }else{
   // Fallback until T2 is deployed: the list projection's coarse waiting state.
   if(w.snapshot.state==='ACTION_REQUIRED')add({key:JSON.stringify(['ask',w.threadId,w.snapshot.turnId,w.snapshot.itemId]),group:'ANSWER',title:w.label,source,text:messagePreview(w.snapshot.text)||t('有一项请求等你确认或回答'),at:time(w.lastMessageAt),action:t('去处理'),avatar:{id:`w:${w.threadId}`,kind:kindOf(w.threadId)},watch:w,parentLabel:parent?.label});
   for(const n of nodes)if((n.state==='WAITING_DECISION'||n.state==='WAITING_REPLY')&&!watched.has(childNodeKey(n.sessionId)))add({key:JSON.stringify(['ask-child',w.threadId,n.sessionId,n.state,n.updatedAt]),group:'ANSWER',title:n.title,source:t('{0} 的子对话',w.label),text:t(n.state==='WAITING_REPLY'?'子对话等你选择':'子对话等你确认'),at:time(n.updatedAt),action:t('去处理'),avatar:{id:`w:${childNodeKey(n.sessionId)}`,kind:'claude'},watch:w,childId:n.sessionId});
  }
  if(w.snapshot.state==='FAILED'){const key=failedAttentionKey(w);if(!isDismissed(key))add({key:JSON.stringify(['failed',key]),group:'FAILED',title:w.label,source,text:messagePreview(w.snapshot.text)||t('执行失败'),at:time(w.lastMessageAt),action:t('查看'),avatar:{id:`w:${w.threadId}`,kind:kindOf(w.threadId)},watch:w,parentLabel:parent?.label,dismissKey:key});}
  for(const n of nodes as ClaudeNode[])if(n.state==='FAILED'&&!watched.has(childNodeKey(n.sessionId))){const key=claudeChildFailureKey(w.threadId,n);if(!isDismissed(key))add({key:JSON.stringify(['failed',key]),group:'FAILED',title:n.title,source:t('{0} 的子对话',w.label),text:t('子对话执行失败'),at:time(n.updatedAt),action:t('查看'),avatar:{id:`w:${childNodeKey(n.sessionId)}`,kind:'claude'},watch:w,childId:n.sessionId,dismissKey:key});}
 }
 for(const item of bridges){
  if(item.lifecycle!=='ACTIVE')continue;
  const entry=activity[item.id];const row=entry?.row;const age=entry?Math.max(0,activityClock-entry.receivedAt):Infinity;
  const [tone,state]=bridgeTone(row,age,item);
  if(tone==='wait'&&state===t('等你确认'))add({key:JSON.stringify(['bridge-ask',item.id]),group:'ANSWER',title:item.name,source:'Bridge',text:t('一端的对话等你确认或回答'),at:time(row?.lastMessageAt??item.lastMessageAt),action:t('去处理'),avatar:{id:`b:${item.id}`,kind:'bridge'},bridgeId:item.id});
  if(row?.readyHandoffs){
   // Transport T4: actionable handoffs under the current binding only.
   const live=row.readyHandoffs.filter(h=>h.bindingRevision===row.bindingRevision);
   const approved=live.filter(h=>h.status==='APPROVED').length,review=live.length-approved;
   if(live.length)add({key:JSON.stringify(['send',item.id,live.map(h=>h.id).sort()]),group:'SEND',title:item.name,source:'Bridge',text:[review?tc('{0} 项结果等你审阅后转发',review):'',approved?tc('{0} 项已批准，等你确认发送',approved):''].filter(Boolean).join(' · '),at:time(row.lastMessageAt??item.lastMessageAt),action:t('审阅并转发'),avatar:{id:`b:${item.id}`,kind:'bridge'},bridgeId:item.id});
  }else if(tone==='wait'&&state===t('有新回复'))add({key:JSON.stringify(['send',item.id]),group:'SEND',title:item.name,source:'Bridge',text:t('有新回复，等你审阅是否转发'),at:time(row?.lastMessageAt??item.lastMessageAt),action:t('去审阅'),avatar:{id:`b:${item.id}`,kind:'bridge'},bridgeId:item.id});
 }
 const order:Record<TodoGroup,number>={OWNER:0,ANSWER:1,SEND:2,FAILED:3};
 return [...cards.values()].sort((a,b)=>order[a.group]-order[b.group]||b.at-a.at||(a.key<b.key?-1:1));
}
