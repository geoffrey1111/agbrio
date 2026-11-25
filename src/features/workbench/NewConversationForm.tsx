import {useEffect,useId,useState} from 'react';
import {readModels} from './readModelCache';
import type {CodexWatch} from './CodexNotifications';
import type {ConversationCreationApi,CreationInput,CreationOptions,CreationReceipt} from './conversationCreationApi';
const key=()=>`agbrio.conversation-creation.v1:${readModels.presentationScope()}`;
function retained():CreationInput|null{try{const v=JSON.parse(localStorage.getItem(key())??'null');return v&&typeof v.requestId==='string'&&typeof v.text==='string'&&typeof v.directory==='string'&&typeof v.provider==='string'?v:null;}catch{return null;}}
export function NewConversationForm({api,onCreated}:{api:ConversationCreationApi;onCreated:(watch:CodexWatch)=>void}){
 const listId=useId();const [pending,setPending]=useState<CreationInput|null>(retained);
 const [provider,setProvider]=useState(pending?.provider??'CODEX'),[directory,setDirectory]=useState(pending?.directory??''),[model,setModel]=useState(pending?.model??''),[effort,setEffort]=useState(pending?.effort??''),[text,setText]=useState(pending?.text??'');
 const [options,setOptions]=useState<CreationOptions|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[receipt,setReceipt]=useState<CreationReceipt|null>(null);
 const [optionsBusy,setOptionsBusy]=useState(true),[optionsError,setOptionsError]=useState(''),[refresh,setRefresh]=useState(0);
 useEffect(()=>{
  let alive=true,timer:ReturnType<typeof setTimeout>|undefined,attempt=0;
  const load=async()=>{
   setOptionsBusy(true);let retry=false;
   try{const value=await api.options();if(!alive)return;setOptions(value);setOptionsError('');retry=value.providers.some(p=>!p.available&&p.reason!=='CLAUDE_DESKTOP_NATIVE_CREATION_PENDING');}
   catch{if(!alive)return;setOptionsError('平台选项暂时读取失败，正在重试；已填写的内容会保留。');retry=true;}
   finally{if(alive)setOptionsBusy(false);}
   if(alive&&retry&&attempt<5){attempt++;timer=setTimeout(()=>void load(),Math.min(8000,2000*attempt));}
  };
  void load();return()=>{alive=false;if(timer)clearTimeout(timer);};
 },[api,refresh]);
 const selected=options?.providers.find(p=>p.id===provider),models=selected?.models??(provider==='CODEX'?options?.models??[]:[]),efforts=models.find(m=>m.model===model)?.supportedReasoningEfforts??[];
 function apply(value:CreationReceipt){setReceipt(value);if(value.status==='COMPLETE'&&value.result?.watch){localStorage.removeItem(key());setPending(null);onCreated(value.result.watch);}}
 // Only read the admitted request. A transient HTTP failure cannot create a
 // new request or resend the first instruction, including after a remount.
 useEffect(()=>{
  if(!pending)return;
  let alive=true,timer:ReturnType<typeof setTimeout>|undefined,failures=0;const started=Date.now();
  const poll=async()=>{
   try{const value=await api.receipt(pending.requestId);if(!alive)return;failures=0;apply(value);
    if(!['EXECUTING','CREATED'].includes(value.status))return;
   }catch{if(!alive)return;failures++;if(failures>=3)setError('回执读取暂时不稳定，正在重连。首条指令不会重复发送。');}
   if(alive&&Date.now()-started<90_000)timer=setTimeout(()=>void poll(),Math.min(8000,1500*2**failures));
  };
  timer=setTimeout(()=>void poll(),1500);
  return()=>{alive=false;if(timer)clearTimeout(timer);};
 },[api,pending?.requestId]);
 async function check(){if(!pending||busy)return;setBusy(true);setError('');try{apply(await api.receipt(pending.requestId));}catch{setError('尚未读到这次创建的回执，请稍后再检查。不会重新创建或重发指令。');}finally{setBusy(false);}}
 async function create(){
  if(busy||pending||!selected?.available||!directory.trim()||!text.trim())return;
  const input:CreationInput={requestId:crypto.randomUUID(),provider,directory:directory.trim(),model:model||null,effort:effort||null,text};
  try{localStorage.setItem(key(),JSON.stringify(input));}catch{setError('无法保存创建记录，尚未发送。请检查本设备存储。');return;}
  setPending(input);setBusy(true);setError('');
  try{apply(await api.create(input));}catch(e){
   if(String(e).includes('_NOT_STARTED')){localStorage.removeItem(key());setPending(null);setError('创建前检查未通过，尚未开始。请核对平台、项目和首条指令。');}
   else setError('创建结果尚未确认。请检查原回执，不要重复创建。');
  }finally{setBusy(false);}
 }
 const locked=busy||Boolean(pending);
 return <form className="v6-bridge-form v6-new-conversation-form" onSubmit={e=>{e.preventDefault();void create();}}>
  <div className="v6-creation-fields">
  <label>Agent 平台<select aria-label="Agent 平台" value={provider} disabled={locked||!options} onChange={e=>{setProvider(e.target.value);setModel('');setEffort('');}}>{(options?.providers??[{id:'CODEX',label:'Codex',available:false}]).map(p=><option key={p.id} value={p.id} >{p.label}{p.reason==='CLAUDE_DESKTOP_NATIVE_CREATION_PENDING'?'（暂不支持新建）':p.available?'':'（暂不可用）'}</option>)}</select></label>
  <div className="v6-creation-readiness" aria-live="polite">
   {optionsBusy&&<p role="status">正在读取平台和模型…</p>}
   {optionsError&&<p role="alert">{optionsError}</p>}
   {selected&&!selected.available&&<p role="status">{selected.reason==='CLAUDE_DESKTOP_NATIVE_CREATION_PENDING'?'Claude Desktop 暂不支持从此处新建；已有对话仍可正常收发消息。':'当前平台尚未就绪，可重新读取选项。'}{selected.reason&&selected.reason!=='CLAUDE_DESKTOP_NATIVE_CREATION_PENDING'&&<small>（{selected.reason}）</small>}</p>}
   <button type="button" disabled={optionsBusy||Boolean(pending)} onClick={()=>setRefresh(v=>v+1)}>重新读取平台和模型</button>
  </div>
  <label>项目目录<input list={listId} aria-label="项目目录" value={directory} disabled={locked} placeholder="选择已有项目，或输入电脑上的项目目录" onChange={e=>setDirectory(e.target.value)}/><datalist id={listId}>{options?.projects.map(p=><option key={p} value={p}/>)}</datalist></label>
  <label>模型<select aria-label="模型" value={model} disabled={locked||!selected?.available} onChange={e=>{setModel(e.target.value);setEffort('');}}><option value="">使用平台默认模型</option>{models.map(m=><option key={m.model} value={m.model}>{m.displayName||m.model}</option>)}</select></label>
  <label>推理强度<select aria-label="推理强度" value={effort} disabled={locked||!model||!efforts.length} onChange={e=>setEffort(e.target.value)}><option value="">使用模型默认强度</option>{efforts.map(e=><option key={e.reasoningEffort} value={e.reasoningEffort}>{e.reasoningEffort}</option>)}</select></label>
  <label>第一条指令<textarea aria-label="第一条指令" value={text} maxLength={100000} disabled={locked} rows={5} onChange={e=>setText(e.target.value)}/></label>
  <p className="v6-muted">发送后跟随原生对话的标题更新。平台暂不可用时不会创建替代对话。</p>
  {error&&<p role="alert" className="v6-error">{error}</p>}
  </div>
  {pending?<><p role="status">{busy?'正在核实创建结果…':receipt?.nativeId?'原对话已创建，首条指令的送达状态仍需核实。':'这次创建已提交，正在等待原回执。'}不会自动重发。</p><button type="button" className="v6-primary" disabled={busy} onClick={()=>void check()}>检查本次创建回执</button></>:<button type="submit" className="v6-primary" disabled={busy||!selected?.available||!directory.trim()||!text.trim()}>创建并发送</button>}
 </form>;
}
