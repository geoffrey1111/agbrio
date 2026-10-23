import {t as uiText,useLanguage} from "../../i18n";
import ReactMarkdown, {defaultUrlTransform,type Components} from "react-markdown";
import remarkGfm from "remark-gfm";
import {Children,createElement,isValidElement,memo,useContext,useEffect,useState,useMemo,useRef,type ReactNode} from "react";
import {Check,Copy} from "lucide-react";
import {MediaPreviewBoundary,MediaPreviewContext} from "./MediaPreview";
import {FilePreview} from "./FilePreview";
import type {MediaResolver} from "./messageMedia";

interface Props {
  text: string;
  media?:MediaResolver;
}

function nodeText(node:ReactNode):string{return Children.toArray(node).map(value=>typeof value==='string'||typeof value==='number'?String(value):isValidElement<{children?:ReactNode}>(value)?nodeText(value.props.children):'').join('');}
function headingId(node:ReactNode){return nodeText(node).trim().toLowerCase().replace(/[^\p{L}\p{N} _-]/gu,'').replace(/\s/g,'-');}
function imageSource(source:string){if(/^https:\/\//i.test(source))return source;if(/^data:image\/(png|jpeg|gif|webp);base64,/i.test(source)&&source.length<=12*1024*1024)return source;return null;}
function isLocal(source:string){return !/^(https?:|mailto:|#)/i.test(source);}
function MediaImage({source,alt,media}:{source:string;alt:string;media?:MediaResolver}){
 useLanguage();
 const openPreview=useContext(MediaPreviewContext);
 const[uri,setUri]=useState<string|null>(()=>imageSource(source));const[failed,setFailed]=useState(false);
 useEffect(()=>{let alive=true;setFailed(false);const direct=imageSource(source);setUri(direct);if(!direct&&media)void media(source).then(file=>{if(!file.mime.startsWith("image/"))throw Error("not an image");if(alive)setUri(`data:${file.mime};base64,${file.data}`);}).catch(()=>{if(alive)setFailed(true);});else if(!direct)setFailed(true);return()=>{alive=false;};},[source,media?.scopeKey??media]);
 return <span className="v5-message-image">{uri&&!failed?<button type="button" aria-label={uiText("查看图片：{0}", alt||uiText("图片"))} onClick={()=>openPreview?.({uri,alt})}><img src={uri} alt={alt||uiText("图片")} loading="lazy" crossOrigin="anonymous" referrerPolicy="no-referrer" onError={()=>setFailed(true)}/></button>:<span>{failed?uiText("图片暂不可用：{0}", alt||source):uiText("图片加载中…")}{failed&&media&&isLocal(source)&&<button type="button" onClick={()=>void media(source,true).then(file=>{if(!file.mime.startsWith("image/"))return download(source,media);setUri(`data:${file.mime};base64,${file.data}`);setFailed(false);}).catch(()=>setFailed(true))}>{uiText("查看本地文件")}</button>}{failed&&/^https:\/\//i.test(source)&&<a href={source} target="_blank" rel="noreferrer noopener">{uiText("打开原图")}</a>}</span>}</span>;
}
function download(source:string,media:MediaResolver){return media(source,true).then(file=>{const bytes=Uint8Array.from(atob(file.data),c=>c.charCodeAt(0));const href=URL.createObjectURL(new Blob([bytes],{type:file.mime}));const a=document.createElement("a");a.href=href;a.download=file.filename;a.click();setTimeout(()=>URL.revokeObjectURL(href),10000);});}

function CodeBlock({children}:{children:ReactNode}){
 useLanguage();
 const source=useRef<HTMLPreElement>(null),timer=useRef<ReturnType<typeof setTimeout>|null>(null);
 const[copied,setCopied]=useState(false),[failure,setFailure]=useState(false);
 useEffect(()=>()=>{if(timer.current)clearTimeout(timer.current);},[]);
 async function copy(){try{await navigator.clipboard.writeText(source.current?.textContent??"");setCopied(true);setFailure(false);if(timer.current)clearTimeout(timer.current);timer.current=setTimeout(()=>setCopied(false),1600);}catch{setFailure(true);}}
 return <div className="r2-code"><div className="r2-code-toolbar"><span>{uiText("代码")}</span><button type="button" aria-label={copied?uiText("已复制代码"):uiText("复制代码")} title={copied?uiText("已复制"):uiText("复制代码")} onClick={()=>void copy()}>{copied?<Check size={18}/>:<Copy size={18}/>}</button></div><pre ref={source}>{children}</pre>{failure&&<span role="status">{uiText("复制未成功，可选择文本复制。")}</span>}</div>;
}

/**
 * Codex output is untrusted document content. Raw HTML remains disabled by
 * react-markdown's default parser, outbound links are isolated, and images are
 * rendered with anonymous/no-referrer image requests or exact-source local media.
 */
export const MarkdownMessage = memo(function MarkdownMessage({ text,media }: Props) {
 useLanguage();
  const[fileError,setFileError]=useState("");
  const[previewSource,setPreviewSource]=useState<string|null>(null);
  const content=useRef<HTMLDivElement>(null);
  const components=useMemo<Components>(()=>({
          ...Object.fromEntries([1,2,3,4,5,6].map(level=>[`h${level}`,({children}:{children?:ReactNode})=>createElement(`h${level}`,{id:headingId(children)},children)])),
          code:({children,className})=>{const path=nodeText(children);return !className&&!path.includes('\n')&&/^(?:[a-z]:[\\/]|\.{0,2}\/)?[^<>]+\.(?:md|txt|csv|json|png|jpg|jpeg|gif|webp|rs|ts|tsx|js|jsx|py|cs|css|html|yaml|yml|toml|log|xml|sql|pdf|xlsx)(?:#L\d+|:\d+(?::\d+)?)?$/i.test(path)&&media?<code><a href="#" onClick={e=>{e.preventDefault();setPreviewSource(path);}}>{children}</a></code>:<code className={className}>{children}</code>;},
          table:({children})=><div className="r2-table-scroll" role="region" tabIndex={0} aria-label={uiText("表格，可左右滚动")}><table>{children}</table></div>,
          pre:({children})=><CodeBlock>{children}</CodeBlock>,
          a: ({ href, children }) => href?.startsWith("#")?<a href={href} onClick={e=>{e.preventDefault();let id=href.slice(1);try{id=decodeURIComponent(id);}catch{}Array.from(content.current?.querySelectorAll("[id]")??[]).find(node=>node.id===id)?.scrollIntoView?.({block:"start"});}}>{children}</a>:href&&isLocal(href)?<a href="#" onClick={e=>{e.preventDefault();if(media)setPreviewSource(href);else setFileError(uiText("附件预览暂不可用。"));}}>{children}</a>:<a href={href} target="_blank" rel="noreferrer noopener">{children}</a>,
          img: ({ alt, src }) => <MediaImage source={typeof src==="string"?src:""} alt={alt??""} media={media}/>,
  }),[media?.scopeKey??media]);
  return (
    <MediaPreviewBoundary><div className="markdown-message" ref={content}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={(url,key)=>key==="src"||isLocal(url)?url:defaultUrlTransform(url)}
        components={components}
      >
        {text}
      </ReactMarkdown>
      {previewSource&&media&&<FilePreview source={previewSource} media={media} close={()=>setPreviewSource(null)}/>}
      {fileError&&<p role="status">{uiText(fileError)}</p>}
    </div></MediaPreviewBoundary>
  );
}, (previous,next)=>previous.text===next.text&&(previous.media===next.media||Boolean(previous.media?.scopeKey&&previous.media.scopeKey===next.media?.scopeKey)));
