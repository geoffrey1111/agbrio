import {PHONE_LAYOUT_REVISION} from "./interfaceRevision";

type Rect={top:number;left:number;width:number;height:number};
type ViewportEvidence={frame:Rect;reported:Rect;resolved:Rect;safe:{top:number;right:number;bottom:number;left:number};standalone:boolean};
let viewport:ViewportEvidence|null=null;
const sheets=new Map<string,unknown>();
const round=(value:number)=>Math.round(value*100)/100;
const box=(element:Element|null)=>{
 if(!element)return null;
 const r=element.getBoundingClientRect();
 return {top:round(r.top),left:round(r.left),width:round(r.width),height:round(r.height)};
};
export function recordViewportEvidence(value:ViewportEvidence){viewport=value;}
/** Geometry only: never read message text, inputs, attributes with target IDs or auth. */
export function recordSheetEvidence(title:string){
 const sheet=document.querySelector('.v4-chat-sheet');
 if(!sheet)return;
 sheets.set(title,{frame:box(document.querySelector('.v4-chat-sheet-frame')),sheet:box(sheet),body:box(sheet.querySelector('.v4-chat-sheet-body')),buttons:[...sheet.querySelectorAll('button')].map(button=>({box:box(button)})),viewport});
}
export function displayDiagnostics(){
 const root=document.documentElement,computed=getComputedStyle(root);
 const manifest=document.querySelector('meta[name="aiwr-shell-assets"]')?.getAttribute('content');
 const worker=navigator.serviceWorker?.controller?.scriptURL;
 const workerBuild=worker?new URL(worker,location.href).searchParams.get('build'):null;
 const visible=window.visualViewport;
 const iosStatusBar=document.querySelector('meta[name="apple-mobile-web-app-status-bar-style"]')?.getAttribute('content')??null;
 const edge=document.getElementById('aiwr-standalone-top-edge');
 const edgeStyle=edge?getComputedStyle(edge):null;
 const topEdge=edgeStyle?{box:box(edge),position:edgeStyle.position,pointerEvents:edgeStyle.pointerEvents,backgroundClip:edgeStyle.backgroundClip,color:edgeStyle.backgroundColor}:null;
 const css=Object.fromEntries(['--aiwr-shell-height','--aiwr-shell-top','--aiwr-shell-bottom','--aiwr-safe-area-top','--aiwr-safe-area-bottom','--aiwr-inset-top','--aiwr-inset-bottom'].map(name=>[name,computed.getPropertyValue(name).trim()]));
 return JSON.stringify({revision:PHONE_LAYOUT_REVISION,iosStatusBar,topEdge,assetManifest:manifest&&/^\/assets\/[A-Za-z0-9_-]+\.json$/.test(manifest)?manifest:null,workerBuild,userAgent:navigator.userAgent,standalone:Boolean((navigator as Navigator&{standalone?:boolean}).standalone)||Boolean(window.matchMedia?.('(display-mode: standalone)').matches),screen:{width:screen.width,height:screen.height},window:{width:innerWidth,height:innerHeight,scrollY:window.scrollY,documentHeight:root.clientHeight,documentScrollHeight:root.scrollHeight,body:box(document.body)},visualViewport:visible?{width:visible.width,height:visible.height,top:visible.offsetTop,left:visible.offsetLeft,scale:visible.scale}:null,viewport,css,layout:{shell:box(document.querySelector('.r2-router')),header:box(document.querySelector('.r2-header')),navigation:box(document.querySelector('.r2-mobile-navigation'))},lastSheets:Object.fromEntries(sheets)},null,2);
}
