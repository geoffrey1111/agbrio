import type {CodexWatch} from './CodexNotifications';
import type {ClaudeTaskTree} from './watchChatApi';
import {failedAttentionKey} from './attentionDismissals';
import {t} from '../../i18n';
import {providerOwnerAttention} from './providerAttention';

/** One presentation tree for everything a parent conversation delegated:
 * Claude Code native descendants (exact native parent IDs) and Pi tasks
 * (server-reviewed association). Relationships are never inferred here. */
export type ChildTone='wait'|'fail'|'run'|'new'|'unknown'|'done';
export type ClaudeNode=ClaudeTaskTree['nodes'][number];
export type ChildEntry={key:string;kind:'claude'|'pi';title:string;depth:number;tone:ChildTone;label:string;at:number;closed:boolean;ended:boolean;node?:ClaudeNode;watch?:CodexWatch};
export type ChildTreeModel={active:ChildEntry[];ended:ChildEntry[];total:number;wait:number;run:number;fail:number;closed:number;limited:boolean;unknown:number};

export const CHILD_FOLD_AGE=86400000;
const rank:Record<ChildTone,number>={wait:0,fail:1,run:2,new:3,unknown:4,done:5};
const claudeLabels:Record<string,string>={RUNNING:'执行中',WAITING_DECISION:'等你确认',WAITING_REPLY:'等你选择',FAILED:'执行失败',INTERRUPTED:'已中断',TURN_ENDED:'本轮已结束',IDLE:'空闲',UNKNOWN:'状态待确认',LIMITED:'状态待确认',ARCHIVED:'已归档'};
const piLabels:Record<string,string>={RUNNING:'执行中',ACTION_REQUIRED:'等你确认',FAILED:'执行失败',RESULT_READY:'结果已到达',INTERRUPTED:'已停止',IDLE:'等待任务'};
export const childNodeKey=(sessionId:string)=>`claude_code:${sessionId}`;
export const claudeChildFailureKey=(parent:string,node:ClaudeNode)=>JSON.stringify(['child-failure',parent,node.sessionId,node.updatedAt]);

type Ctx={now:number;isDismissed:(id:string)=>boolean;seen:(id:string,at:number|null|undefined)=>boolean;parentThreadId:string};
function claudeEntry(node:ClaudeNode,ctx:Ctx):ChildEntry{
 const key=childNodeKey(node.sessionId),at=node.updatedAt||0;
 let tone:ChildTone=node.state==='RUNNING'?'run':node.state==='WAITING_DECISION'||node.state==='WAITING_REPLY'?'wait':node.state==='FAILED'?'fail':node.state==='UNKNOWN'||node.state==='LIMITED'?'unknown':'done';
 if(tone==='fail'&&ctx.isDismissed(claudeChildFailureKey(ctx.parentThreadId,node)))tone='done';
 const ended=tone==='done'&&(ctx.seen(key,at)||ctx.now-at>CHILD_FOLD_AGE);
 return {key,kind:'claude',title:node.title,depth:node.depth,tone,label:t(claudeLabels[node.state]??'状态待确认'),at,closed:false,ended,node};
}
export function piEntry(w:CodexWatch,ctx:Omit<Ctx,'parentThreadId'>):ChildEntry{
 const at=w.lastMessageAt??w.changedAt??0,state=w.snapshot.state,closed=w.piLifecycle?.state==='CLOSED';
 let tone:ChildTone;let label=t(piLabels[state]??'状态待确认');
 if(providerOwnerAttention(w.errorCode)){tone=ctx.isDismissed(failedAttentionKey(w))?'done':'wait';label=t('需要你处理');}
 else if(w.errorCode){tone='unknown';label=t('暂时无法读取');}
 else if(state==='ACTION_REQUIRED'||(w.pending?.length??0)>0){tone='wait';label=t('等你确认');}
 else if(state==='FAILED')tone=ctx.isDismissed(failedAttentionKey(w))?'done':'fail';
 else if(state==='RUNNING'&&!closed)tone='run';
 else if(state==='RESULT_READY'&&!ctx.seen(w.threadId,at)&&w.unreadResult!==false&&(w.unreadResult===true||ctx.now-at<=CHILD_FOLD_AGE))tone='new';
 else tone='done';
 if(closed&&tone==='done')label=state==='FAILED'?t('执行失败 · 已关闭'):t('已关闭');
 const ended=tone==='done'&&(closed||ctx.seen(w.threadId,at)||ctx.now-at>CHILD_FOLD_AGE);
 return {key:w.threadId,kind:'pi',title:w.label,depth:1,tone,label,at,closed,ended,watch:w};
}

/** Orders needs-you first, then running, unseen results, the rest; a native
 * subtree moves with its most urgent member and keeps its own hierarchy. */
export function buildChildTree({parentThreadId,tree,pi,now=Date.now(),isDismissed,seen,exclude}:{parentThreadId:string;tree?:ClaudeTaskTree|null;pi:CodexWatch[];now?:number;isDismissed:(id:string)=>boolean;seen:(id:string,at:number|null|undefined)=>boolean;exclude?:Set<string>}):ChildTreeModel{
 const ctx={now,isDismissed,seen,parentThreadId};
 type Group={entries:ChildEntry[];rank:number;at:number;ended:boolean};
 const nodes=(tree?.nodes??[]).filter(n=>!exclude?.has(childNodeKey(n.sessionId)));
 const ids=new Set(nodes.map(n=>n.sessionId));
 const kids=new Map<string,ClaudeNode[]>();
 for(const n of nodes){const parent=ids.has(n.parentSessionId)?n.parentSessionId:'';kids.set(parent,[...(kids.get(parent)??[]),n]);}
 const entries=new Map(nodes.map(n=>[n.sessionId,claudeEntry(n,ctx)]));
 const memo=new Map<string,{rank:number;at:number}>();
 const score=(n:ClaudeNode):{rank:number;at:number}=>{const hit=memo.get(n.sessionId);if(hit)return hit;const own=entries.get(n.sessionId)!;let best={rank:rank[own.tone],at:own.at};for(const c of kids.get(n.sessionId)??[]){const s=score(c);if(s.rank<best.rank||s.rank===best.rank&&s.at>best.at)best={rank:s.rank,at:Math.max(best.at,s.at)};else best={...best,at:Math.max(best.at,s.at)};}memo.set(n.sessionId,best);return best;};
 const order=(a:{rank:number;at:number;key:string},b:{rank:number;at:number;key:string})=>a.rank-b.rank||b.at-a.at||(a.key<b.key?-1:a.key>b.key?1:0);
 const flatten=(n:ClaudeNode,depth:number,out:ChildEntry[])=>{out.push({...entries.get(n.sessionId)!,depth});for(const c of [...(kids.get(n.sessionId)??[])].map(c=>({c,...score(c),key:c.sessionId})).sort(order))flatten(c.c,depth+1,out);};
 const groups:(Group&{key:string})[]=[];
 for(const root of kids.get('')??[]){const out:ChildEntry[]=[];flatten(root,1,out);const s=score(root);groups.push({entries:out,rank:s.rank,at:s.at,ended:out.every(e=>e.ended),key:root.sessionId});}
 for(const w of pi){if(exclude?.has(w.threadId))continue;const e=piEntry(w,ctx);groups.push({entries:[e],rank:rank[e.tone],at:e.at,ended:e.ended,key:w.threadId});}
 groups.sort(order);
 const all=groups.flatMap(g=>g.entries);
 return {
  active:groups.filter(g=>!g.ended).flatMap(g=>g.entries),
  ended:groups.filter(g=>g.ended).flatMap(g=>g.entries),
  total:all.length,wait:all.filter(e=>e.tone==='wait').length,run:all.filter(e=>e.tone==='run').length,fail:all.filter(e=>e.tone==='fail').length,
  closed:all.filter(e=>e.closed).length,limited:Boolean(tree?.limited),unknown:all.filter(e=>e.tone==='unknown').length,
 };
}
/** "子对话 8 · 执行中 2 · 失败 1" */
export function childTreeSummary(m:ChildTreeModel){
 const parts=[t('子对话 {0}',m.total)];
 if(m.wait)parts.push(t('等你 {0}',m.wait));
 if(m.run)parts.push(t('执行中 {0}',m.run));
 if(m.fail)parts.push(t('失败 {0}',m.fail));
 return parts.join(' · ');
}
