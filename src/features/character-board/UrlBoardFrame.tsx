import {useEffect,useMemo,useRef,useState} from 'react';
import {ExternalLink,RefreshCw} from 'lucide-react';
import {t} from '../../i18n';
import {sizeBoardInputs} from './mobileInputSizing';
import {bootstrapRevision,probeBoardRevision} from './boardFrameRevision';
/** URL boards remain origin-isolated. Only same-origin presentation controls are observed. */
export function UrlBoardFrame({src,active,internalOnly=false}:{src:string;active:boolean;internalOnly?:boolean}){
 const frame=useRef<HTMLIFrameElement>(null),scroll=useRef(0),dirty=useRef(false),hash=useRef(''),detach=useRef(()=>{}),lastLoad=useRef(0),away=useRef(false),inactiveSince=useRef<number|null>(null),revision=useRef<string|null>(null),lastProbe=useRef(0),probe=useRef<AbortController|null>(null),search=useRef<string|null>(null);
 const [attempt,setAttempt]=useState(0),[checking,setChecking]=useState(false),[failed,setFailed]=useState(false),[updateAvailable,setUpdateAvailable]=useState(false);
 const timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
 const visible=useRef(active);visible.current=active;
 const url=useMemo(()=>{if(!attempt)return src;const u=new URL(src);if(search.current!==null)u.search=search.current;if(hash.current)u.hash=hash.current;u.searchParams.set('_agbrio_refresh',String(attempt));return u.href;},[src,attempt]);
 function refresh(){try{scroll.current=frame.current?.contentWindow?.scrollY??scroll.current;hash.current=frame.current?.contentWindow?.location.hash??hash.current;const current=frame.current?.contentWindow?.location;if(current?.pathname===new URL(src).pathname)search.current=current.search;}catch{/* foreign frame */}probe.current?.abort();detach.current();dirty.current=false;setUpdateAvailable(false);setChecking(true);setFailed(false);lastLoad.current=Date.now();setAttempt(n=>Math.max(Date.now(),n+1));clearTimeout(timer.current);timer.current=setTimeout(()=>{setChecking(false);setFailed(true);},15000);}
 const refreshLatest=useRef(refresh);refreshLatest.current=refresh;
 useEffect(()=>{
  if(!active){inactiveSince.current??=Date.now();return;}
  const check=async()=>{
   const target=new URL(src);if(!internalOnly||target.origin!==location.origin||!/^\/v1\/mobile\/review-boards\/[a-f0-9]{32}\/frame\/$/.test(target.pathname)||!revision.current||Date.now()-lastProbe.current<60000)return;
   lastProbe.current=Date.now();const controller=new AbortController();probe.current=controller;const timeout=setTimeout(()=>controller.abort(),8000);
   try{const next=await probeBoardRevision(src,controller.signal);if(!controller.signal.aborted&&next&&next!==revision.current)setUpdateAvailable(true);}catch{/* Retain the loaded board on network/auth failure; never claim it is fresh. */}finally{clearTimeout(timeout);if(probe.current===controller)probe.current=null;}
  };
  const resume=()=>{if(document.hidden){inactiveSince.current??=Date.now();return;}const since=inactiveSince.current;inactiveSince.current=null;
   // Only an explicit external-browser visit keeps the historical reload path.
   // Switching workbench tabs preserves this exact iframe/document and inputs.
   if(away.current&&!dirty.current&&Date.now()-lastLoad.current>1000){away.current=false;refreshLatest.current();return;}
   if(since!==null){try{frame.current?.contentWindow?.scrollTo(0,scroll.current);}catch{/* foreign frame */}}
   if(since!==null&&Date.now()-since>=30000)void check();
  };
  resume();window.addEventListener('focus',resume);window.addEventListener('pageshow',resume);document.addEventListener('visibilitychange',resume);
  return()=>{probe.current?.abort();window.removeEventListener('focus',resume);window.removeEventListener('pageshow',resume);document.removeEventListener('visibilitychange',resume);};
 },[active,src,internalOnly]);
 useEffect(()=>()=>{probe.current?.abort();detach.current();clearTimeout(timer.current);},[]);
 function loaded(e:React.SyntheticEvent<HTMLIFrameElement>){if(e.currentTarget!==frame.current)return;clearTimeout(timer.current);setChecking(false);setFailed(false);lastLoad.current=Date.now();sizeBoardInputs(e.currentTarget);try{const doc=e.currentTarget.contentDocument,win=e.currentTarget.contentWindow;if(!doc||!win)return;const origin=new URL(src).origin;if(win.location.origin!==origin)return;revision.current=bootstrapRevision(doc.getElementById('board-bootstrap')?.textContent??'');
 const input=(event:Event)=>{const target=event.target as Element|null;if(target?.matches('input[type=search],#search,#filter'))return;if(event.isTrusted||doc.activeElement===target)dirty.current=true;};const click=(event:Event)=>{const target=event.target as Element|null;if(target?.closest?.('#reload')){event.preventDefault();event.stopImmediatePropagation();refreshLatest.current();}};
 const rememberScroll=()=>{if(visible.current&&!document.hidden)scroll.current=win.scrollY;};win.addEventListener('scroll',rememberScroll,{passive:true});
 doc.addEventListener('input',input);doc.addEventListener('change',input);doc.addEventListener('click',click,true);detach.current=()=>{win.removeEventListener('scroll',rememberScroll);doc.removeEventListener('input',input);doc.removeEventListener('change',input);doc.removeEventListener('click',click,true);};if(attempt)win.scrollTo(0,scroll.current);
 }catch{/* Do not read or inject a foreign board. Its outer refresh still works. */}}
 return <div className="board-web"><iframe key={attempt} ref={frame} src={url} title={t('看板')} onLoad={loaded} sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-downloads" referrerPolicy="no-referrer"/>{failed&&<p role="status" className="board-web-refresh-error">{t('看板刷新未完成，请重试。')}</p>}<button type="button" className="board-web-refresh" aria-label={t(updateAvailable?'看板有更新，刷新查看':'刷新看板')} title={t(updateAvailable?'看板有更新，刷新查看':'刷新看板')} data-update-available={updateAvailable||undefined} aria-busy={checking} disabled={checking} onClick={refresh}><RefreshCw size={18} className={checking?'v4-progress-spinner':undefined}/></button>{!internalOnly&&<a className="board-web-open" href={src} target="_blank" rel="noreferrer noopener" onClick={()=>{away.current=true;}}><ExternalLink size={16}/>{t('在浏览器打开')}</a>}</div>;
}
