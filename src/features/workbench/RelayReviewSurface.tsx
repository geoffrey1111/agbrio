import {t as uiText,useLanguage,getLanguage} from "../../i18n";
import * as Dialog from "@radix-ui/react-dialog";
import {useEffect,useState,type ReactNode} from "react";
import {ArrowRight,ArrowUp,Check,ChevronLeft,CircleCheck,Ellipsis,Eye,Pencil,Trash2} from "lucide-react";
import {MarkdownMessage} from "../codex/MarkdownMessage";
import type {MediaResolver} from "../codex/messageMedia";
import {RelayBlockSelection} from "./RelayBlockSelection";
import type {RoleReview} from "./roleReviewDrafts";
import {CopyAction} from "./NativeReaderActions";

export function RelayReviewSurface({review,busy,oldReview,sourceLabel,targetLabel,media,error,connectionAction,change,attachmentChange,submit,close,discard,checkStatus}:{
 review:RoleReview;busy:boolean;oldReview:boolean;sourceLabel:string;targetLabel:string;media:MediaResolver;
 error:string|null;connectionAction:ReactNode;change:(value:RoleReview)=>void;
 attachmentChange:(ids:string[])=>void;submit:()=>void;close:()=>void;discard:()=>void;checkStatus:()=>void;
}){
 useLanguage();
 const [view,setView]=useState<"BLOCKS"|"EDIT"|"PREVIEW">(review.approved?"PREVIEW":review.blocks?.length?"BLOCKS":"EDIT");
 const[menu,setMenu]=useState(false);
 const frozen=Boolean(review.approved)||oldReview;
 const status=review.approved?.status;
 const [receiptReady,setReceiptReady]=useState(false);
 useEffect(()=>{setReceiptReady(false);if(status!=="SENT")return;const timer=setTimeout(()=>setReceiptReady(true),350);return()=>clearTimeout(timer);},[status]);
 useEffect(()=>{if(review.approved)setView("PREVIEW");},[review.approved?.id]);
 const settled=status&&!["READY","APPROVED"].includes(status);
 const count=review.blockIds?.length??0;
 const payload=review.choosing&&review.blocks?.length?(count===review.blocks.length?review.reply.text:review.blocks.filter(block=>review.blockIds?.includes(block.id)).map(block=>block.text).join("\n\n")):review.text;
 const previewPayload=payload+(review.attachmentManifest??"");
 const canSend=!busy&&!oldReview&&!settled&&payload.trim()&&(!review.choosing||count>0)&&(!review.selected.length||review.prepared);
 const receipt=status==="CANCELLED"?uiText("已结束"):status==="SENT"?uiText("已发送"):status==="SENDING"?uiText("发送中"):status==="UNKNOWN"?uiText("送达待确认"):status==="FAILED"?uiText("发送未完成"):null;
 return <>
  <header className="r2-relay-header"><button type="button" className="r2-icon" aria-label={uiText("关闭审阅")} disabled={busy} onClick={close}><ChevronLeft size={20}/></button><Dialog.Title>{uiText("转发")}</Dialog.Title><CopyAction text={previewPayload} label={uiText("复制发送内容")}/><button type="button" className="r2-icon" aria-label={uiText("草稿操作")} aria-expanded={menu} disabled={busy} onClick={()=>setMenu(!menu)}><Ellipsis size={20}/></button></header>
  {menu&&<div className="r2-draft-menu"><button type="button" onClick={()=>{setView("PREVIEW");setMenu(false);}}>{uiText("预览发送内容")}</button><button type="button" onClick={()=>{setMenu(false);discard();}}><Trash2 size={18}/>{uiText("放弃本地编辑")}</button></div>}
  <div className="r2-relay-route"><span title={sourceLabel}>{review.role==="DECISION"?uiText("控制端"):uiText("执行端")} · {sourceLabel}</span><ArrowRight size={18}/><strong title={targetLabel}>{review.role==="DECISION"?uiText("执行端"):uiText("控制端")} · {targetLabel}</strong></div>
  <div className="r2-relay-scroll">
    {review.reply.observedAt&&<time className="r2-relay-time">{new Intl.DateTimeFormat(getLanguage(),{month:"numeric",day:"numeric",hour:"2-digit",minute:"2-digit"}).format(review.reply.observedAt)}</time>}
    {view==="BLOCKS"&&review.blocks?.length?<RelayBlockSelection blocks={review.blocks} selected={review.blockIds??[]} disabled={busy||frozen} media={media} onChange={ids=>{
      const ordered=review.blocks!.filter(block=>ids.includes(block.id)).map(block=>block.id);
      const unchanged=review.appliedBlockIds&&ordered.length===review.appliedBlockIds.length&&ordered.every((id,index)=>id===review.appliedBlockIds![index]);
      change({...review,blockIds:ordered,appliedBlockIds:ordered,choosing:false,text:unchanged?review.text:ordered.length===review.blocks!.length?review.reply.text:review.blocks!.filter(block=>ordered.includes(block.id)).map(block=>block.text).join("\n\n"),selected:unchanged?review.selected:[],prepared:unchanged?review.prepared:undefined,attachmentManifest:unchanged?review.attachmentManifest:undefined});
    }}/>:view==="EDIT"&&!frozen?<label className="r2-relay-editor"><span className="r2-sr-only">{uiText("发送内容")}</span><textarea autoFocus aria-label={uiText("跨端交接发送内容")} value={payload} disabled={busy} onChange={event=>change({...review,text:event.target.value,choosing:false})}/></label>:<div className="r2-relay-preview"><MarkdownMessage text={previewPayload} media={media}/></div>}
    {review.options.length>0&&<section className="r2-relay-files" aria-label={uiText("跨端附件选择")}><h3>{uiText("附件")}</h3>{review.attachmentManifest&&<p className="r2-attachment-hint">{uiText("确认发送时复制到接收端项目，Codex 按路径读取。")}</p>}{review.options.map(file=><label key={file.id}><input type="checkbox" disabled={busy||frozen||!payload.trim()} checked={review.selected.includes(file.id)} onChange={event=>attachmentChange(event.target.checked?[...review.selected,file.id]:review.selected.filter(id=>id!==file.id))}/>{file.filename}</label>)}</section>}
    {review.prepared?.attachments.length? <div className="r2-relay-files">{review.prepared.attachments.map(file=><small key={file.id}>{file.filename} · {file.size} {uiText("字节")}</small>)}</div>:null}
    {oldReview&&<p role="status">{uiText("绑定已更改 · 此草稿仅供查看")}</p>}
    {receipt&&<p role="status" className="r2-relay-receipt"><Check size={18}/>{uiText(receipt)}</p>}
    {error&&<p role="alert">{uiText(error)}</p>}{connectionAction}
  </div>
  <footer className="r2-relay-dock"><div><span aria-live="polite"><CircleCheck size={16}/>{count} {uiText("段")}</span><button type="button" className="r2-icon" aria-label={view==="EDIT"&&review.blocks?.length?uiText("选择段落"):uiText("编辑发送内容")} aria-pressed={view==="EDIT"} disabled={busy||frozen||!payload.trim()} onClick={()=>{if(view==="EDIT"&&review.blocks?.length){setView("BLOCKS");return;}if(review.choosing)change({...review,text:payload,choosing:false});setView("EDIT");}}>{view==="EDIT"&&review.blocks?.length?<Check size={20}/>:<Pencil size={20}/>}</button><button type="button" className="r2-icon" aria-label={uiText("预览发送内容")} aria-pressed={view==="PREVIEW"} disabled={busy||!payload.trim()} onClick={()=>setView("PREVIEW")}><Eye size={20}/></button></div>
    {settled?<button type="button" className="r2-action" disabled={busy||(status==="SENT"&&!receiptReady)} onClick={status==="SENT"||status==="CANCELLED"?close:checkStatus}>{status==="SENT"||status==="CANCELLED"?uiText("返回对话"):uiText("检查送达状态")}</button>:<button type="button" className="r2-action" aria-label={uiText("确认并发送给{0}", review.role==="DECISION"?uiText("执行端"):uiText("控制端"))} disabled={!canSend} onClick={submit}><ArrowUp size={20}/>{busy?uiText("提交中…"):uiText("确认发送")}</button>}
  </footer>
 </>;
}
