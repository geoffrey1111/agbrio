import {Goal,ChevronDown} from "lucide-react";
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
export function CodexGoalStatus({goal}:{goal:MobileCodexGoal}){
 const time=goalElapsed(goal.timeUsedSeconds);
 return <details className="r2-codex-goal" data-status={goal.status}><summary><Goal size={15} aria-hidden/><span className="r2-goal-phase">{goalStatusText(goal.status)}</span><span className="r2-goal-objective" title={goal.objective}>{goal.objective}</span>{time&&<time>{time}</time>}<ChevronDown size={14} aria-hidden/></summary><div className="r2-goal-detail"><p>{goal.objective}</p></div></details>;
}
