import {t as uiText,useLanguage} from "../../i18n";
import {createContext,useContext,useEffect,useRef,useState,type ReactNode} from "react";
import {Check,Copy} from "lucide-react";
export const NativeReaderActions=createContext<{register:(text:string|null)=>void}|null>(null);
export function useReaderCopy(text:string|null){const context=useContext(NativeReaderActions);useEffect(()=>{context?.register(text);return()=>context?.register(null);},[context,text]);}
export function CopyAction({text,label=uiText("复制全文"),onError}:{text:string|null;label?:string;onError?:()=>void}){
 useLanguage();
 const[copied,setCopied]=useState(false),[failed,setFailed]=useState(false),timer=useRef<ReturnType<typeof setTimeout>|null>(null);
 useEffect(()=>()=>{if(timer.current)clearTimeout(timer.current);},[]);
 async function copy(){try{await navigator.clipboard.writeText(text??"");setFailed(false);setCopied(true);if(timer.current)clearTimeout(timer.current);timer.current=setTimeout(()=>setCopied(false),1600);}catch{setFailed(true);onError?.();}}
 return <><button type="button" className="r2-icon" aria-label={copied?uiText("已复制"):uiText(label)} title={copied?uiText("已复制"):uiText(label)} disabled={!text} onClick={()=>void copy()}>{copied?<Check size={20}/>:<Copy size={20}/>}</button>{failed&&!onError&&<span className="r2-copy-error" role="status">{uiText("复制未成功，可选择正文复制。")}</span>}</>;
}
export function NativeCopyProvider({register,children}:{register:(text:string|null)=>void;children:ReactNode}){
 useLanguage();const value=useRef({register});return <NativeReaderActions.Provider value={value.current}>{children}</NativeReaderActions.Provider>;}
