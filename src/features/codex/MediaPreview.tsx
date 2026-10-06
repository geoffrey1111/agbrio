import {t as uiText,useLanguage} from "../../i18n";
import {createContext,useCallback,useContext,useEffect,useRef,useState,type ReactNode} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {Maximize2,Plus,X} from "lucide-react";
import {useBackLayer} from "../workbench/navigationHistory";

type Preview={uri:string;alt:string};
export const MediaPreviewContext=createContext<((preview:Preview)=>void)|null>(null);
/** A snapshot survives later reply refreshes; resolving files still belongs to MediaResolver. */
export function MediaPreviewProvider({children}:{children:ReactNode}){
 useLanguage();
 const[preview,setPreview]=useState<Preview|null>(null),[zoom,setZoom]=useState(1);
 useBackLayer(Boolean(preview),()=>setPreview(null));
 const[area,setArea]=useState({width:0,height:0}),[natural,setNatural]=useState<{width:number;height:number}|null>(null);
 const viewport=useRef<HTMLDivElement>(null);
 const observer=useRef<ResizeObserver|null>(null);
 const open=useCallback((value:Preview)=>{setZoom(1);setNatural(null);setPreview(value);},[]);
 const attachViewport=useCallback((element:HTMLDivElement|null)=>{observer.current?.disconnect();viewport.current=element;if(!element)return;const resize=()=>{const r=element.getBoundingClientRect();setArea({width:r.width||window.innerWidth,height:r.height||window.innerHeight});};resize();if(typeof ResizeObserver!=="undefined"){observer.current=new ResizeObserver(resize);observer.current.observe(element);}},[]);
 useEffect(()=>{const resize=()=>{const element=viewport.current;if(!element)return;const r=element.getBoundingClientRect();setArea({width:r.width||window.innerWidth,height:r.height||window.innerHeight});};window.addEventListener("resize",resize);return()=>{observer.current?.disconnect();window.removeEventListener("resize",resize);};},[]);
 const fit=natural?Math.min(1,Math.max(1,area.width-32)/natural.width,Math.max(1,area.height-32)/natural.height):1;
 const imageWidth=natural?natural.width*fit*zoom:undefined,imageHeight=natural?natural.height*fit*zoom:undefined;
 useEffect(()=>{if(zoom===1)viewport.current?.scrollTo?.({left:0,top:0});},[zoom]);
 return <MediaPreviewContext.Provider value={open}>{children}<Dialog.Root open={Boolean(preview)} onOpenChange={value=>{if(!value)setPreview(null);}}><Dialog.Portal><Dialog.Overlay className="r2-image-overlay"/><Dialog.Content className="r2-image-viewer" aria-describedby={undefined}><header><Dialog.Title>{preview?.alt||uiText("图片")}</Dialog.Title><Dialog.Close asChild><button type="button" aria-label={uiText("关闭图片")}><X size={20}/></button></Dialog.Close></header><div ref={attachViewport} className="r2-image-viewport" tabIndex={0} aria-label={uiText("图片预览，可滚动查看")}><div className="r2-image-canvas" style={{width:Math.max(area.width,(imageWidth??0)+32),height:Math.max(area.height,(imageHeight??0)+32)}}>{preview&&<img src={preview.uri} alt={preview.alt||uiText("图片")} crossOrigin="anonymous" referrerPolicy="no-referrer" onLoad={event=>{const image=event.currentTarget;if(image.naturalWidth&&image.naturalHeight)setNatural({width:image.naturalWidth,height:image.naturalHeight});}} style={{width:imageWidth,height:imageHeight,maxWidth:imageWidth?"none":undefined,maxHeight:imageHeight?"none":undefined}}/>}</div></div><footer><button type="button" aria-label={uiText("缩小图片")} disabled={zoom===1} onClick={()=>setZoom(value=>Math.max(1,value-.5))}><span aria-hidden>−</span></button><button type="button" aria-label={uiText("适应屏幕")} onClick={()=>setZoom(1)}><Maximize2 size={20}/></button><button type="button" aria-label={uiText("放大图片")} disabled={zoom===3} onClick={()=>setZoom(value=>Math.min(3,value+.5))}><Plus size={20}/></button></footer></Dialog.Content></Dialog.Portal></Dialog.Root></MediaPreviewContext.Provider>;
}
export function MediaPreviewBoundary({children}:{children:ReactNode}){
 useLanguage();const existing=useContext(MediaPreviewContext);return existing?<>{children}</>:<MediaPreviewProvider>{children}</MediaPreviewProvider>;}
