import {useEffect,useId,useRef,useState} from "react";
import {ChevronLeft,Check,X,MessageCircleQuestion} from "lucide-react";
import {t as uiText,useLanguage} from "../../i18n";
import type {MobileCodexRequest} from "../../mobile/api";

export function CodexRequestCard({request,disabled,respond}:{request:MobileCodexRequest;disabled:boolean;respond:(input:object)=>Promise<void>}){
 useLanguage();const group=useId(),answerKey=`aiwr-request-answer:${request.requestId}:${request.revision}`,questions=request.questions??[],secret=questions.some(q=>q.isSecret);
 const [answers,setAnswers]=useState<Record<string,string>>(()=>{if(secret)return {};try{return JSON.parse(sessionStorage.getItem(answerKey)??"{}");}catch{return {};}});
 const [index,setIndex]=useState(0),[expanded,setExpanded]=useState(true),[sending,setSending]=useState(false);const lock=useRef(false);
 useEffect(()=>{if(secret)return;try{sessionStorage.setItem(answerKey,JSON.stringify(answers));}catch{/* Keep in memory. */}},[answerKey,answers,secret]);
 async function submit(skip=false){if(lock.current||disabled||request.responseSent)return;lock.current=true;setSending(true);try{await respond(skip?{revision:request.revision,decision:"skip"}:{revision:request.revision,answers});}finally{lock.current=false;setSending(false);}}
 const q=questions[index],blocked=disabled||sending||request.responseSent,complete=questions.every(q=>!q.required||answers[q.id]?.trim()),selected=q?answers[q.id]??"":"";
 if(request.kind!=="USER_INPUT")return <section className="v4-chat-request"><h3>{uiText("Codex 需要你确认")}</h3>{request.reason&&<p>{request.reason}</p>}<div className="v4-chat-actions">{request.choices?.map(c=><button type="button" key={c.id} disabled={blocked} onClick={()=>{if(lock.current)return;lock.current=true;setSending(true);void respond({revision:request.revision,decision:c.id}).finally(()=>{lock.current=false;setSending(false);});}}>{uiText(c.id==="accept"?"允许这次请求":c.id==="decline"?"拒绝":c.id==="cancel"?"取消":c.label)}</button>)}</div></section>;
 if(request.responseSent)return <p className="v4-question-receipt" role="status"><Check size={16}/>{uiText("已答复，等待 Codex 确认。")}</p>;
 if(!expanded)return <button type="button" className="v4-answer-question" onClick={()=>setExpanded(true)}><MessageCircleQuestion size={16}/>{uiText("回答问题")}</button>;
 return <section className="v4-chat-request v4-question-card" aria-label={uiText("Codex 执行请求")}>
  <header><span><MessageCircleQuestion size={16}/>{q?.label||uiText("问题")}{questions.length>1&&<small>{index+1}/{questions.length}</small>}</span><button type="button" aria-label={uiText("收起问题")} disabled={sending} onClick={()=>setExpanded(false)}><X size={18}/></button></header>
  {request.reason&&<p>{request.reason}</p>}
  {q&&<div className="v4-question-scroll"><fieldset disabled={blocked}><legend>{q.placeholder||q.label}</legend>
   {!!q.options?.length&&<div className="v4-question-choices">{q.options.map((option,i)=><label key={i} data-selected={selected===option.label}><input type="radio" name={`${group}-${q.id}`} value={option.label} checked={selected===option.label} onChange={()=>setAnswers(old=>({...old,[q.id]:option.label}))}/><span><strong>{option.label}</strong>{option.description&&<> <small>{option.description}</small></>}</span></label>)}</div>}
   {<label className="v4-question-freeform"><span className="v5-sr-only">{uiText("回答{0}",q.label)}</span>{q.isSecret?<input type="password" autoComplete="off" value={selected} placeholder={uiText("回复…")} onChange={e=>setAnswers(old=>({...old,[q.id]:e.target.value}))}/>:<textarea aria-label={uiText("回答{0}",q.label)} value={q.options?.some(o=>o.label===selected)?"":selected} placeholder={uiText("回复…")} rows={2} onChange={e=>setAnswers(old=>({...old,[q.id]:e.target.value}))}/>}</label>}
  </fieldset></div>}
  <footer>{questions.length>1&&<div><button type="button" aria-label={uiText("上一题")} disabled={blocked||index===0} onClick={()=>setIndex(i=>i-1)}><ChevronLeft size={18}/></button></div>}<span/><button type="button" disabled={blocked} onClick={()=>void submit(true)}>{uiText("跳过")}</button>{index<questions.length-1?<button type="button" disabled={blocked||!selected.trim()} onClick={()=>setIndex(i=>i+1)}>{uiText("下一题")}</button>:<button type="button" className="v3-primary" disabled={blocked||!complete||!questions.length} onClick={()=>void submit()}>{uiText(sending?"正在发送":"发送")}</button>}</footer>
 </section>;
}
