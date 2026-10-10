import {useEffect,useState} from "react";
import {Goal,ChevronDown,Pause,Play,LoaderCircle} from "lucide-react";
import {t} from "../../i18n";
import type {MobileCodexGoal} from "../../mobile/api";
// Verified against the registered Codex Desktop26.1002.6548.0 locale and native
// ThreadGoal schema. A blocked Goal is stalled, not completed or a failed turn.
const labels:Record<string,string>={active:"进行中的目标",paused:"已暂停的目标",blocked:"目标已停滞",budgetLimited:"目标受限",usageLimited:"目标使用受限",complete:"已达成目标"};
export function goalStatusText(status:string){return t(labels[status]??"目标状态待确认");}
export function goalElapsed(seconds:number|null|undefined){
 if(typeof seconds!=="number"||!Number.isFinite(seconds)||seconds<0)return null;
 const total=Math.floor(seconds),h=Math.floor(total/3600),m=Math.floor(total%3600/60),s=total%60;
 return `${h?h+'h ':''}${m?m+'m ':''}${s}s`;
}
export function displayedGoalSeconds(goal:MobileCodexGoal,now:number,checkedAt?:number){
 const used=goal.timeUsedSeconds;if(typeof used!=="number"||!Number.isFinite(used)||used<0)return null;
 if(goal.status!=="active"||typeof goal.updatedAt!=="number"||!goal.updatedAt||!checkedAt)return used;
 const updated=goal.updatedAt<1e12?goal.updatedAt*1000:goal.updatedAt;
 // Match native display interpolation without writing derived usage back. A
 // disconnected/stale Host stops advancing after its freshness grace period.
 return used+Math.max(0,Math.min(now,checkedAt+15000)-updated)/1000;
}
export function CodexGoalStatus({goal,checkedAt,busy=false,onPause,onResume}:{goal:MobileCodexGoal;checkedAt?:number;busy?:boolean;onPause?:()=>void;onResume?:()=>void}){
 const [now,setNow]=useState(Date.now);
 useEffect(()=>{if(goal.status!=="active")return;let timer:ReturnType<typeof setInterval>|undefined;const sync=()=>{if(timer)clearInterval(timer);setNow(Date.now());if(document.visibilityState!=="hidden")timer=setInterval(()=>setNow(Date.now()),1000);};sync();document.addEventListener("visibilitychange",sync);return()=>{if(timer)clearInterval(timer);document.removeEventListener("visibilitychange",sync);};},[goal.status,goal.fingerprint]);
 const time=goalElapsed(displayedGoalSeconds(goal,now,checkedAt));
 return <div className="r2-goal-controls-wrapper"><details className="r2-codex-goal" data-status={goal.status}><summary><Goal size={15} aria-hidden/><span className="r2-goal-phase">{goalStatusText(goal.status)}</span><span className="r2-goal-objective" title={goal.objective}>{goal.objective}</span>{time&&<time>{time}</time>}<ChevronDown size={14} aria-hidden/></summary><div className="r2-goal-detail"><p>{goal.objective}</p>{typeof goal.tokensUsed==="number"&&<p className="r2-goal-accounting">{t("已用 {0} tokens",goal.tokensUsed)}{typeof goal.tokenBudget==="number"?` / ${goal.tokenBudget}`:""}</p>}</div></details>{(onPause||onResume)&&<button type="button" className="r2-goal-control" disabled={busy} aria-label={t(onPause?"暂停目标":"继续目标")} title={t(onPause?"暂停目标":"继续目标")} onClick={onPause??onResume}>{busy?<LoaderCircle size={16} aria-hidden/>:onPause?<Pause size={16} aria-hidden/>:<Play size={16} aria-hidden/>}</button>}</div>;
}
