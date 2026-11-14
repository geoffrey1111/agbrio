import {useMemo} from 'react';
import {CircleCheck,LayoutGrid,ChevronRight} from 'lucide-react';
import {t,tc,useLanguage} from '../../i18n';
import {dismissAttention,useDismissedAttention} from './attentionDismissals';
import {useWatches} from './sharedWatches';
import {deriveTodoCards,type TodoCard,type TodoGroup} from './todoCards';
import {ConversationAvatar,when,type WatchOpenTarget} from './ConversationHome';
import {markResultSeen} from './seenResults';
import type {CodexWatch,NotificationApi} from './CodexNotifications';
import type {BridgeActivity} from './bridgeActivity';
import type {WorkbenchItem} from './models';

type ActivityRows=Record<string,{row:BridgeActivity;receivedAt:number}>;
type Inputs={watchApi?:NotificationApi;bridges:WorkbenchItem[];activity:ActivityRows;activityClock:number};
/** The single to-do derivation: the page and the navigation count use it. */
export function useTodoCards({watchApi,bridges,activity,activityClock}:Inputs){
 const [watches]=useWatches(watchApi);
 const isDismissed=useDismissedAttention();
 const active=bridges.filter(b=>b.lifecycle==='ACTIVE');
 return deriveTodoCards({watches,bridges:active,activity,activityClock,isDismissed});
}
const groupTitle:Record<TodoGroup,string>={OWNER:'需要你处理',ANSWER:'等你回答',SEND:'待确认的发送',FAILED:'执行失败'};
export function TodoPage({onOpenBridge,onOpenWatch,onOpenBoard,onOpenLog,countInHeader=false,...inputs}:Inputs&{onOpenBridge:(id:string)=>void;onOpenWatch:(watch:CodexWatch,target?:WatchOpenTarget)=>void;onOpenBoard?:()=>void;onOpenLog?:()=>void;/** The host shows the count beside the page title. */countInHeader?:boolean}){
 useLanguage();
 const cards=useTodoCards(inputs);
 const groups=useMemo(()=>(['OWNER','ANSWER','SEND','FAILED'] as TodoGroup[]).map(g=>[g,cards.filter(c=>c.group===g)] as const).filter(([,rows])=>rows.length),[cards]);
 const open=(c:TodoCard)=>{if(c.bridgeId)onOpenBridge(c.bridgeId);else if(c.watch){if(c.childId)onOpenWatch(c.watch,{childId:c.childId});else{markResultSeen(c.watch.threadId,c.watch.lastMessageAt);if(c.parentLabel)onOpenWatch(c.watch,{parentLabel:c.parentLabel});else onOpenWatch(c.watch);}}};
 return <section className="v7-todo" aria-label={t('待办')}>
  {!countInHeader&&<p className="v7-todo-count" role="status">{cards.length?tc('{0} 件事等你处理',cards.length):t('没有等你处理的事')}</p>}
  {groups.map(([group,rows])=><section key={group} className="v7-todo-group" aria-label={t(groupTitle[group])}>
   <h2>{t(groupTitle[group])} · {rows.length}</h2>
   {rows.map(c=><article key={c.key} className="v7-todo-card" data-group={c.group}>
    <header><ConversationAvatar id={c.avatar.id} kind={c.avatar.kind}/><span className="v7-todo-source"><strong>{c.title}</strong><span> · {c.source}</span></span>{c.at?<time>{when(c.at)}</time>:null}</header>
    <p className="v7-todo-text">{c.text}</p>
    {c.detail&&<p className="v7-todo-detail">{t('上游原话：')}<q>{c.detail}</q></p>}
    {c.next&&<p className="v7-todo-next">{t('下一步：')}{c.next}</p>}
    <footer>{c.dismissKey&&<button type="button" className="v7-todo-quiet" onClick={()=>dismissAttention(c.dismissKey!)}>{t('知道了')}</button>}<button type="button" className="v7-todo-go" aria-label={`${c.action}：${c.title}`} onClick={()=>open(c)}>{c.action}</button></footer>
   </article>)}
  </section>)}
  {!cards.length&&<div className="v7-todo-empty"><CircleCheck size={30} aria-hidden/><h2>{t('没有等你处理的事')}</h2><p>{t('有提问、待确认的发送或执行失败时，会出现在这里。')}</p></div>}
  <nav className="v7-todo-links" aria-label={t('相关入口')}>
   {onOpenBoard&&<button type="button" onClick={onOpenBoard}><LayoutGrid size={16} aria-hidden/><span>{t('看板审批')}</span><ChevronRight size={16} aria-hidden/></button>}
   {onOpenLog&&<button type="button" onClick={onOpenLog}><span>{t('通知记录')}</span><ChevronRight size={16} aria-hidden/></button>}
  </nav>
  <p className="v7-todo-hint">{t('处理完或已失效的事会自动消失。结果通知不在这里：新结果在「对话」里用小圆点标出，完整记录在 设置 › 通知 › 通知记录。')}</p>
 </section>;
}
