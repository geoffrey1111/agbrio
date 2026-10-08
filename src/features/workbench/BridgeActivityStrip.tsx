import {useEffect,useRef} from "react";
import {LoaderCircle,Check,MessageCircle,Pause,TriangleAlert,Clock3} from "lucide-react";
import {t} from "../../i18n";
import {goalStatusText} from "./CodexGoalStatus";
import {focusedBridgeActivity,type ActivityPhase,type BridgeActivity} from "./bridgeActivity";
const copy:Record<ActivityPhase,string>={GOAL_ACTIVE:"进行中的目标",THINKING:"正在思考",RUNNING:"正在执行",ACTION_REQUIRED:"等待确认",COMPLETE:"最新回复",RESULT_PENDING:"正在读取结果",EMPTY:"",PAUSED:"已暂停",LIMITED:"目标受限",FAILED:"执行失败",INTERRUPTED:"已停止",UNCONFIRMED:"状态待确认"};
export function BridgeActivityStrip({activity,age}:{activity:BridgeActivity;age:number}){
 const root=useRef<HTMLSpanElement>(null),previous=useRef<Record<string,string>>({});
 const focus=focusedBridgeActivity(activity,age);
 const phases=focus?`${focus.side.role}:${focus.side.phase}:${focus.concurrent}`:"idle";
 useEffect(()=>{
  const node=root.current;if(!node)return;let stopped=false,dispose=()=>{};const before=previous.current;
  previous.current=Object.fromEntries([...node.querySelectorAll<HTMLElement>(".r2-agent-state")].map(n=>[n.dataset.role!,n.dataset.phase!]));
  void import("gsap").then(({gsap})=>{if(stopped)return;const media=gsap.matchMedia();
   media.add("(prefers-reduced-motion: no-preference)",()=>{
    const timelines:ReturnType<typeof gsap.timeline>[]=[];
    for(const state of node.querySelectorAll<HTMLElement>(".r2-agent-state")){
     const phase=state.dataset.phase,symbol=state.querySelector(".r2-agent-symbol"),signal=state.querySelector(".r2-agent-signal");
     if(["RUNNING","THINKING"].includes(phase!)){timelines.push(gsap.timeline({repeat:-1}).to(symbol,{rotation:360,duration:1.1,ease:"none"}));timelines.push(gsap.timeline({repeat:-1,yoyo:true}).to(signal,{opacity:.35,scale:.65,duration:.7,ease:"sine.inOut"}));}
     if(phase==="COMPLETE"&&["RUNNING","THINKING","RESULT_PENDING"].includes(before[state.dataset.role!]??""))timelines.push(gsap.timeline().fromTo(symbol,{scale:.85},{scale:1.12,duration:.15,ease:"power2.out"}).to(symbol,{scale:1,duration:.22,ease:"power3.out",clearProps:"transform"}));
    }
    let visible=true;const update=()=>{for(const timeline of timelines){if(visible&&document.visibilityState!=="hidden")timeline.resume();else timeline.pause();}};
    const observer=typeof IntersectionObserver==="undefined"?null:new IntersectionObserver(entries=>{visible=entries[0]?.isIntersecting??false;update();});observer?.observe(node);document.addEventListener("visibilitychange",update);update();
    return()=>{observer?.disconnect();document.removeEventListener("visibilitychange",update);};
   });dispose=()=>media.revert();
  }).catch(()=>{});
  return()=>{stopped=true;dispose();};
 },[phases]);
 if(!focus)return null;
 const {side,concurrent}=focus,phase=side.phase;
 const Icon=phase==="COMPLETE"?Check:phase==="ACTION_REQUIRED"?MessageCircle:phase==="PAUSED"?Pause:phase==="FAILED"?TriangleAlert:phase==="RESULT_PENDING"?Clock3:LoaderCircle;
 const text=concurrent?t("两端同时执行"):side.goalStatus&&!["RUNNING","THINKING"].includes(phase)&&!(side.goalStatus==="active"&&phase==="ACTION_REQUIRED")?goalStatusText(side.goalStatus):t(copy[phase]);
 return <span ref={root} className="r2-bridge-statuses" aria-label={t("当前状态")}><span className="r2-agent-state" data-phase={phase} data-role={side.role}>
 <span className="r2-agent-symbol"><Icon size={13}/><i className="r2-agent-signal" aria-hidden/></span>{!concurrent&&<span>{t(side.role==="DECISION"?"控制":"执行")}</span>}<strong>{text}</strong>
 </span></span>;
}
