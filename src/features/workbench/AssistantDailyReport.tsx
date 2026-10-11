import {useState} from "react";
import {getLanguage,useLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";
import {dailyReportInstructions,type DailyReportRequest} from "./dailyReportInstructions";
export type ReportBridge={id:string;name:string;bindings:{bindingRevision:number}};
export function AssistantDailyReport({bridges=[]}:{bridges?:ReportBridge[]}){
 useLanguage();const language=getLanguage();const text=(cn:string,en:string,tw:string)=>language==="en"?en:language==="zh-TW"?tw:cn;
 const[open,setOpen]=useState(false),[accepted,setAccepted]=useState(false),[time,setTime]=useState("22:00"),[zone,setZone]=useState(()=>Intl.DateTimeFormat().resolvedOptions().timeZone),[destination,setDestination]=useState(""),[selected,setSelected]=useState<string[]>([]),[action,setAction]=useState<DailyReportRequest["action"]>("UPSERT");
 let validZone=false;try{new Intl.DateTimeFormat("en",{timeZone:zone});validZone=zone.trim().length>0;}catch{/* Invalid zones never become schedule requests. */}
 const chosen=bridges.filter(b=>selected.includes(b.id));
 const prompt=accepted&&validZone&&/^\d{2}:\d{2}$/.test(time)&&destination.trim()&&chosen.length?dailyReportInstructions({action,time,timezone:zone,destination,bridges:chosen.map(b=>({id:b.id,bindingRevision:b.bindings.bindingRevision}))}):null;
 return <section className="assistant-daily-report" aria-label={text("日报（可选）","Daily report (optional)","日報（可選）")}>
  <h3>{text("日报（可选）","Daily report (optional)","日報（可選）")}</h3>
  <p>{text("只读验收成功后，把日报请求交给助手。推荐每天本地时间 22:00，可修改或停用。","After read-only acceptance, submit a report request to your assistant. Suggested: 22:00 local time; change or disable anytime.","唯讀驗收成功後，把日報請求交給助手。建議每天本地時間22:00，可修改或停用。")}</p>
  {!open?<div className="assistant-daily-actions"><button type="button" onClick={()=>setOpen(true)}>{text("配置或管理日报","Configure or manage report","配置或管理日報")}</button><span>{text("可跳过 · 当前没有替你创建任务","Optional · no task created here","可跳過 · 目前沒有替你建立任務")}</span></div>:<>
   <label className="assistant-daily-check"><input type="checkbox" checked={accepted} onChange={e=>setAccepted(e.target.checked)}/>{text("助手已回报真实只读验收成功；以下 Bridge 已明确委托","My assistant reported real read-only acceptance; I delegated the Bridges below","助手已回報真實唯讀驗收成功；以下Bridge已明確委託")}</label>
   <p className="assistant-guide-note">{text("此勾选是你的说明，不是应用独立验证连接或已启用日报。","This is your statement, not independent connection proof or an enabled report.","此勾選是你的說明，不是應用獨立驗證連接或已啟用日報。")}</p>
   <div className="assistant-daily-fields">
    <label>{text("操作","Action","操作")}<select value={action} onChange={e=>setAction(e.target.value as DailyReportRequest["action"])}><option value="UPSERT">{text("设置 / 修改（复用既有任务）","Set / change (reuse existing task)","設定 / 修改（復用既有任務）")}</option><option value="PAUSE">{text("停用既有日报","Disable existing report","停用既有日報")}</option></select></label>
    <label>{text("每天本地时间","Daily local time","每天本地時間")}<input type="time" value={time} onChange={e=>setTime(e.target.value)}/></label>
    <label>{text("时区（IANA）","Timezone (IANA)","時區（IANA）")}<input value={zone} maxLength={80} onChange={e=>setZone(e.target.value)} aria-invalid={!validZone} placeholder="Asia/Shanghai"/></label>
    <label>{text("发送到哪里","Deliver to","發送到哪裡")}<input value={destination} maxLength={200} onChange={e=>setDestination(e.target.value)} placeholder={text("例如：当前 dot 的对话","For example: this dot conversation","例如：目前dot的對話")}/></label>
   </div>
   <fieldset><legend>{text("已授权的 Bridge 范围","Delegated Bridge scope","已授權的Bridge範圍")}</legend>{bridges.length?bridges.map(b=><label className="assistant-daily-check" key={b.id}><input type="checkbox" checked={selected.includes(b.id)} onChange={e=>setSelected(old=>e.target.checked?[...old,b.id]:old.filter(id=>id!==b.id))}/><span>{b.name} · rev {b.bindings.bindingRevision}</span></label>):<p>{text("暂无可选 Bridge。先完成绑定与委托。","No Bridges available. Bind and delegate first.","暫無可選Bridge。先完成綁定與委託。")}</p>}</fieldset>
   <p>{text("复制后须明确提交给助手。它会核对定时工具、既有日报和真实保存结果；不支持就说明未启用。OAuth 连接不会自动同意定时任务。","Explicitly submit after copying. Your assistant checks scheduler tools, existing reports and saved results; unsupported means not enabled. OAuth does not consent to scheduled tasks.","複製後須明確提交給助手。它會核對定時工具、既有日報和真實保存結果；不支援就說明未啟用。OAuth連接不會自動同意定時任務。")}</p>
   <div className="assistant-daily-actions"><CopyAction showLabel text={prompt} label={text("复制日报请求","Copy report request","複製日報請求")}/><button type="button" onClick={()=>setOpen(false)}>{text("暂时跳过","Skip for now","暫時跳過")}</button></div>
   {prompt&&<details className="assistant-prompt-preview"><summary>{text("预览日报请求","Preview report request","預覽日報請求")}</summary><pre>{prompt}</pre></details>}
  </>}
 </section>;
}
