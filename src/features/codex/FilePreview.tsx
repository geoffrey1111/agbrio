import {useEffect,useMemo,useRef,useState} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {Download,X} from "lucide-react";
import {t as uiText} from "../../i18n";
import {useBackLayer} from "../workbench/navigationHistory";
import {MarkdownMessage} from "./MarkdownMessage";
import type {MediaFile,MediaResolver} from "./messageMedia";
import "./filePreview.css";

const textExtensions=new Set("md txt csv json rs ts tsx js jsx py cs css html yaml yml toml log xml sql".split(" "));
export function fileExtension(name:string){return name.split(/[?#]/)[0].replace(/:\d+(?::\d+)?$/,'').split('.').pop()?.toLowerCase()??'';}
export function downloadMedia(file:MediaFile){const bytes=Uint8Array.from(atob(file.data),c=>c.charCodeAt(0));const url=URL.createObjectURL(new Blob([bytes],{type:file.mime}));const a=document.createElement('a');a.href=url;a.download=file.filename;a.click();setTimeout(()=>URL.revokeObjectURL(url),10000);}
/** RFC4180 quoted fields, including embedded newlines; bounded before rendering. */
export function csvRows(text:string){
 const rows:string[][]=[];let row:string[]=[],field='',quoted=false,closed=false;
 for(let i=0;i<text.length;i++){const c=text[i];if(c==='"'){if(quoted&&text[i+1]==='"'){field+='"';i++;}else if(quoted){quoted=false;closed=true;}else if(!field&&!closed)quoted=true;else throw Error('CSV_INVALID');}
 else if(!quoted&&(c===','||c==='\n'||c==='\r')){row.push(field);field='';closed=false;if(row.length>100)throw Error('CSV_LIMIT');if(c!==','){if(c==='\r'&&text[i+1]==='\n')i++;rows.push(row);row=[];if(rows.length>2000)throw Error('CSV_LIMIT');}}else{if(closed)throw Error('CSV_INVALID');field+=c;}}
 if(quoted)throw Error('CSV_INVALID');if(field||row.length||closed){row.push(field);rows.push(row);}if(row.length>100||rows.length>2000||rows.reduce((n,r)=>n+r.length,0)>100000)throw Error('CSV_LIMIT');return rows;
}
export function FilePreview({source,media,close}:{source:string|null;media:MediaResolver;close:()=>void}){
 const [file,setFile]=useState<MediaFile|null>(null),[error,setError]=useState(''),[zoom,setZoom]=useState(1);
 const body=useRef<HTMLDivElement>(null);
 useBackLayer(Boolean(source),close);
 useEffect(()=>{let alive=true;if(!source){setFile(null);return;}setFile(null);setError('');setZoom(1);void media(source,true).then(value=>{if(alive)setFile(value);}).catch(()=>{if(alive)setError(uiText('文件暂不可用，请确认文件仍存在且未超过大小限制。'));});return()=>{alive=false;};},[source,media]);
 const decoded=useMemo(()=>{if(!file)return null;const ext=fileExtension(file.filename);if(file.mime.startsWith('image/'))return {kind:'image' as const,text:''};if(!textExtensions.has(ext))return {kind:'download' as const,text:''};try{const bytes=Uint8Array.from(atob(file.data),c=>c.charCodeAt(0));if(bytes.length>1024*1024)return {kind:'download' as const,text:''};let text=new TextDecoder('utf-8',{fatal:true}).decode(bytes);if(text.includes('\0')||text.split('\n').length>20000)return {kind:'download' as const,text:''};if(ext==='json'){try{text=JSON.stringify(JSON.parse(text),null,2);}catch{/* Preserve invalid JSON verbatim. */}}if(text.split('\n').length>20000)return {kind:'download' as const,text:''};return {kind:ext==='md'?'markdown' as const:ext==='csv'?'csv' as const:'text' as const,text};}catch{return {kind:'download' as const,text:''};}},[file]);
 const csv=useMemo(()=>{if(decoded?.kind!=='csv')return null;try{return csvRows(decoded.text);}catch{return null;}},[decoded]);
 const related=useMemo<MediaResolver>(()=>Object.assign((path:string,requested?:boolean,parents?:string[])=>media(path,requested,[...(source?[source]:[]),...(parents??[])]),{scopeKey:`${media.scopeKey??''}:${source}`}),[source,media]);
 useEffect(()=>{if(decoded?.kind==='text'){const line=Number(source?.match(/(?:#L|:)(\d+)(?::\d+)?$/)?.[1]);if(line)body.current?.querySelector(`[data-line="${line}"]`)?.scrollIntoView?.({block:'center'});}else body.current?.scrollTo?.(0,0);},[decoded,source]);
 return <Dialog.Root open={Boolean(source)} onOpenChange={open=>{if(!open)close();}}><Dialog.Portal><Dialog.Overlay className="r2-image-overlay"/><Dialog.Content className="agbrio-file-preview" aria-describedby={undefined}>
  <header><Dialog.Title title={file?.filename}>{file?.filename??uiText('文件预览')}</Dialog.Title>{file&&<button type="button" aria-label={uiText('下载文件')} onClick={()=>downloadMedia(file)}><Download size={20}/></button>}<Dialog.Close asChild><button type="button" aria-label={uiText('关闭预览')}><X size={20}/></button></Dialog.Close></header>
  <div className="agbrio-file-body" ref={body} tabIndex={0}>
   {error?<p role="alert">{error}</p>:!file?<p role="status">{uiText('正在读取文件…')}</p>:decoded?.kind==='image'?<><div className="agbrio-file-image-tools"><button onClick={()=>setZoom(v=>Math.max(1,v-.5))} disabled={zoom===1} aria-label={uiText('缩小图片')}>−</button><button onClick={()=>setZoom(1)}>{uiText('适应屏幕')}</button><button onClick={()=>setZoom(v=>Math.min(4,v+.5))} disabled={zoom===4} aria-label={uiText('放大图片')}>+</button></div><img className="agbrio-file-image" src={`data:${file.mime};base64,${file.data}`} alt={file.filename} style={{width:`${zoom*100}%`,maxWidth:'none'}}/></>:decoded?.kind==='markdown'?<MarkdownMessage text={decoded.text} media={related}/>:decoded?.kind==='csv'&&csv?<table className="agbrio-file-csv"><tbody>{csv.map((row,i)=><tr key={i}>{row.map((cell,j)=><td key={j}>{cell}</td>)}</tr>)}</tbody></table>:decoded?.kind==='text'||decoded?.kind==='csv'?<>{decoded.kind==='csv'&&<p>{uiText('表格较大或格式不完整，显示原文。')}</p>}<pre className="agbrio-file-code">{decoded.text.split('\n').map((line,i)=><span data-line={i+1} key={i}><span aria-hidden="true" className="agbrio-file-line">{i+1}</span>{line}{'\n'}</span>)}</pre></>:<p>{uiText('此文件暂不支持内置预览，可下载后查看。')}</p>}
  </div>
 </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
