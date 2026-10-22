import {ClaudeDecisionCards} from './ClaudeDecisionCards';
import {useEffect,useState} from 'react';
import {MarkdownMessage} from '../codex/MarkdownMessage';
import {chronologicalHistoryPage,mergeHistory} from './chatTimeline';
import type {WatchChatApi,ChatHistory,ChatMessage,ClaudeTaskTree,ClaudeOwnerRequest} from './watchChatApi';
import {t} from '../../i18n';
import './claudeTaskStatus.css';
const labels:Record<string,string>={RUNNING:'正在执行',WAITING_DECISION:'等待确认',WAITING_REPLY:'等待你的选择',FAILED:'执行失败',INTERRUPTED:'已中断',TURN_ENDED:'本轮已结束',IDLE:'空闲',UNKNOWN:'状态待确认',LIMITED:'状态待确认',ARCHIVED:'已归档'};
export function claudeTaskSummary(tree:ClaudeTaskTree){
 if(tree.attention)return t('子对话有 {0} 项需要处理',tree.attention);
 if(tree.running)return t('子对话执行中 · {0} 项',tree.running);
 if(tree.unknown||tree.limited)return t('子对话状态尚未全部确认');
 return tree.nodes.length?t('子对话本轮均已结束'):null;
}
/** Native relationships only. Opening this disclosure never controls a task. */
type Props={tree:ClaudeTaskTree;api?:WatchChatApi;threadId?:string;generation?:number;notificationsEnabled?:boolean;onNotificationsChanged?:()=>Promise<void>;expanded?:boolean;onOpenChild?:(node:ClaudeTaskTree["nodes"][number])=>void};
export function ClaudeChildReader({node,api,threadId,generation,standalone=false}:{node:ClaudeTaskTree['nodes'][number];standalone?:boolean}&Props){
 const [requests,setRequests]=useState<ClaudeOwnerRequest[]>([]);
 const [history,setHistory]=useState<ChatMessage[]>([]),[cursor,setCursor]=useState<string|null>(null),[loaded,setLoaded]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(false),[open,setOpen]=useState(standalone);
 useEffect(()=>{if(open)void load();},[open,node.state,node.updatedAt,generation]);
 useEffect(()=>{if(!open)return;const timer=setInterval(()=>{if(document.visibilityState!=='hidden')void load();},5000);return()=>clearInterval(timer);},[open,api,threadId,generation,node.sessionId]);
 async function load(older=false){if(!api||!threadId||busy)return;setBusy(true);setError(false);try{const page=await api.command<ChatHistory>({action:'CHILD_HISTORY',threadId,generation,childId:node.sessionId,cursor:older?cursor:null});const rows=chronologicalHistoryPage(page.messages);setHistory(old=>older?mergeHistory(rows,old):mergeHistory(old,rows));if(!older){setRequests(page.ownerRequests??[]);setCursor(page.nextCursor);}else setCursor(page.nextCursor);setLoaded(true);}catch{setError(true)}finally{setBusy(false)}}
 const content=<>{api&&threadId&&generation!==undefined&&<ClaudeDecisionCards requests={requests} api={api} threadId={threadId} generation={generation} childId={node.sessionId} refresh={()=>load()}/>}{error?<p role="alert">{t('暂时无法读取子对话。')} <button type="button" onClick={()=>void load()}>{t('重试')}</button></p>:busy&&!loaded?<p role="status">{t('正在加载…')}</p>:loaded&&<div className="claude-child-messages">{cursor&&<button type="button" disabled={busy} onClick={()=>void load(true)}>{t('加载更早消息')}</button>}{history.map(m=>m.kind==='COLLABORATION'?<details key={m.id} className="v4-chat-collaboration"><summary>{t('内部协作记录')}</summary><MarkdownMessage text={m.text}/></details>:<article key={m.id} className={m.role==='user'?'claude-child-user':'claude-child-assistant'}><span>{m.role==='user'?t('你'):'Claude Code'}</span><MarkdownMessage text={m.text}/></article>)}{!history.length&&<p>{t('暂时没有可读取的公开消息。')}</p>}</div>}</>;
 if(standalone)return <section className="claude-child-reader"><p data-state={node.state}>{t(labels[node.state]??'状态待确认')}</p>{content}</section>;
 return <details className="claude-child-result" onToggle={e=>setOpen(e.currentTarget.open)}><summary><span className="claude-task-title">{node.depth>1?'↳ ':''}{node.title}</span><span data-state={node.state}>{t(labels[node.state]??'状态待确认')}</span></summary>{content}</details>

}
export function ClaudeTaskStatus({tree,api,threadId,generation,notificationsEnabled=false,onNotificationsChanged,expanded=false,onOpenChild}:Props){
 const [saving,setSaving]=useState(false),[error,setError]=useState(false);
 async function toggle(enabled:boolean){if(!api||!threadId)return;setSaving(true);setError(false);try{await api.command({action:'CHILD_NOTIFICATIONS',threadId,generation,enabled});await onNotificationsChanged?.();}catch{setError(true)}finally{setSaving(false)}}
 if(!tree.nodes.length&&!tree.limited)return null;
 return <details className="claude-task-tree" open={expanded||undefined}><summary>{claudeTaskSummary(tree)??t('子对话')} · {t('查看任务树')}</summary>
  <p>{t('这里显示各对话的本轮状态，不代表整个工作已完成。')}</p>
  {api&&<label className="claude-child-toggle"><input type="checkbox" checked={notificationsEnabled} disabled={saving} onChange={e=>void toggle(e.target.checked)}/>{t('推送子对话结果')}</label>}
  {api&&<p>{t('默认关闭；开启后仅推送新的结果，沿用已有通知渠道。')}</p>}{error&&<p role="alert">{t('设置未保存，请重试。')}</p>}
  <ul>{tree.nodes.map(node=><li key={node.sessionId} style={{paddingInlineStart:`${Math.min(node.depth-1,7)*16}px`}}>{onOpenChild?<button type="button" className="claude-child-open" onClick={()=>onOpenChild(node)}><span className="claude-task-title">{node.depth>1?"↳ ":""}{node.title}</span><span>{t(labels[node.state]??"状态待确认")} ›</span></button>:api?<ClaudeChildReader node={node} tree={tree} api={api} threadId={threadId} generation={generation}/>:<><span className="claude-task-title">{node.depth>1?'↳ ':''}{node.title}</span><span data-state={node.state}>{t(labels[node.state]??'状态待确认')}</span></>}</li>)}</ul>
  {(tree.unknown>0||tree.limited)&&<p role="status">{t('部分关系或状态暂时无法读取，未将它们视为完成。')}</p>}
 </details>;
}
