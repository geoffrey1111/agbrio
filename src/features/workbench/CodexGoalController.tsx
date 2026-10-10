import {useRef,useState} from 'react';
import {t} from '../../i18n';
import {CodexGoalStatus} from './CodexGoalStatus';
import type {MobileCodexGoal} from '../../mobile/api';
import type {GoalControls,GoalControlReceipt,WatchChatApi,NativeTurnDiagnostic} from './watchChatApi';
const key=(thread:string)=>`aiwr-goal-action:${thread}`;
function pendingId(thread:string){try{const id=sessionStorage.getItem(key(thread));return id&&/^[0-9a-f-]{36}$/i.test(id)?id:null;}catch{return null;}}
export function goalControlError(error:unknown){
 const code=String(error);
 if(code.includes('GOAL_NATIVE_RETRY_PENDING'))return t('Codex 正在自动重试，请等待本轮状态更新。');
 if(/GOAL_PRECHECK_FAILED|GOAL_BACKEND_UNAVAILABLE|GOAL_SESSION_UNAVAILABLE/.test(code))return t('暂时无法连接 Codex，本次目标操作未执行。');
 if(/GOAL_TARGET_CHANGED|GOAL_CHANGED_REFRESH|GOAL_NO_LONGER_EXISTS/.test(code))return t('目标或绑定已变化，请刷新后再操作。');
 if(/PENDING|PREVIOUS_UNCERTAIN/.test(code))return t('先处理待确认问题或原发送记录，再管理目标。');
 if(code.includes('GOAL_TURN_ALREADY_RUNNING'))return t('目标正在执行，不需要再次恢复。');
 return t('目标操作尚未确认，请检查原记录，不会重复执行。');
}
export function nativeTurnErrorText(diagnostic?:NativeTurnDiagnostic|null){
 if(!diagnostic||diagnostic.status!=="failed")return null;
 const labels:Record<string,string>={serverOverloaded:"服务暂时繁忙",rateLimitExceeded:"请求频率受限",usageLimitExceeded:"使用量受限",sessionBudgetExceeded:"本次预算受限",contextWindowExceeded:"对话上下文已满",httpConnectionFailed:"服务连接中断",responseStreamConnectionFailed:"回复连接中断",responseStreamDisconnected:"回复连接中断",unauthorized:"需要检查服务授权"};
 return t(labels[diagnostic.errorCode??""]??"这一轮执行失败，具体原因未提供");
}
export function CodexGoalController({goal,controls,api,checkedAt,diagnostic,disabled=false,refresh}:{goal:MobileCodexGoal;controls?:GoalControls|null;api:WatchChatApi;checkedAt?:number;diagnostic?:NativeTurnDiagnostic|null;disabled?:boolean;refresh:()=>Promise<void>}){
 const[pending,setPending]=useState(()=>pendingId(goal.threadId)),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const[confirmed,setConfirmed]=useState<{previousFingerprint:string|undefined;goal:MobileCodexGoal}|null>(null);
 const shownGoal=confirmed&&goal.fingerprint===confirmed.previousFingerprint?confirmed.goal:goal;
 const running=useRef(false);
 function clear(){setPending(null);try{sessionStorage.removeItem(key(goal.threadId));}catch{/* receipt remains on Host */}}
 async function reconcile(receipt:GoalControlReceipt|null){
  if(receipt&&(receipt.threadId!==goal.threadId||receipt.id!==pending))throw Error('GOAL_TARGET_CHANGED');
  if(receipt?.status==='APPLIED'){if(receipt.result)setConfirmed({previousFingerprint:goal.fingerprint,goal:receipt.result});clear();setError('');await refresh();}
  else if(receipt?.status==='REJECTED'){clear();setError(goalControlError(receipt.errorCode));await refresh();}
  else setError(t('目标操作尚未确认，请检查原记录，不会重复执行。'));
 }
 async function control(operation:'PAUSE_GOAL'|'RESUME_GOAL'){
  if(running.current||disabled||pending||!controls||!shownGoal.fingerprint)return;
  running.current=true;setBusy(true);setError('');const id=crypto.randomUUID();
  try{sessionStorage.setItem(key(goal.threadId),id);}catch{/* current pending id is retained in memory */}setPending(id);
  try{
   const receipt=await api.command<GoalControlReceipt>({action:'GOAL_CONTROL',threadId:goal.threadId,id,operation,input:{target:controls.target,expectedGoalFingerprint:shownGoal.fingerprint},confirmed:true});
   if(receipt.id!==id||receipt.threadId!==goal.threadId)throw Error('GOAL_TARGET_CHANGED');
   if(receipt.status==='APPLIED'){if(receipt.result)setConfirmed({previousFingerprint:goal.fingerprint,goal:receipt.result});clear();await refresh();}else setError(goalControlError(receipt.errorCode));
  }catch(cause){setError(goalControlError(cause));}
  finally{running.current=false;setBusy(false);}
 }
 async function check(){if(running.current||!pending)return;running.current=true;setBusy(true);try{await reconcile(await api.command<GoalControlReceipt|null>({action:'GOAL_RECEIPT',threadId:goal.threadId,id:pending}));}catch(cause){setError(goalControlError(cause));}finally{running.current=false;setBusy(false);}}
 const enabled=!disabled&&!pending&&Boolean(shownGoal.fingerprint);
 return <div className="r2-goal-manager">{nativeTurnErrorText(diagnostic)&&<p className="r2-goal-feedback">{nativeTurnErrorText(diagnostic)}</p>}<CodexGoalStatus goal={shownGoal} checkedAt={checkedAt} busy={busy||!enabled||Boolean(controls?.blockedReason)||Boolean(shownGoal.status!=="active"&&!controls?.canResume)} onPause={Boolean(controls)&&shownGoal.status==="active"?()=>void control('PAUSE_GOAL'):undefined} onResume={Boolean(controls)&&["paused","blocked","usageLimited"].includes(shownGoal.status)?()=>void control('RESUME_GOAL'):undefined}/>{(pending||error)&&<p className="r2-goal-feedback" role="status">{error||t('正在同步目标状态…')}{pending&&!busy&&<button type="button" onClick={()=>void check()}>{t('检查目标操作记录')}</button>}</p>}{controls?.blockedReason&&!pending&&<p className="r2-goal-feedback">{goalControlError(controls.blockedReason)}</p>}</div>;
}
