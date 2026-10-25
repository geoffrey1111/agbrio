import {useEffect,useMemo,useRef,useState} from 'react';
import {ExternalLink,RefreshCw} from 'lucide-react';
import {t} from '../../i18n';
import {sizeBoardInputs} from './mobileInputSizing';
/** URL boards remain origin-isolated. Only same-origin presentation controls are observed. */
export function UrlBoardFrame({src,active}:{src:string;active:boolean}){
 const frame=useRef<HTMLIFrameElement>(null),scroll=useRef(0),dirty=useRef(false),detach=useRef(()=>{}),lastLoad=useRef(0),away=useRef(false),wasActive=useRef(active);
 const [attempt,setAttempt]=useState(0),[checking,setChecking]=useState(false),[failed,setFailed]=useState(false);
 const timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
 const url=useMemo(()=>{if(!attempt)return src;const u=new URL(src);u.searchParams.set('_agbrio_refresh',String(attempt));return u.href;},[src,attempt]);
 function refresh(){try{scroll.current=frame.current?.contentWindow?.scrollY??scroll.current;}catch{/* foreign frame */}detach.current();dirty.current=false;setChecking(true);setFailed(false);lastLoad.current=Date.now();setAttempt(n=>Math.max(Date.now(),n+1));clearTimeout(timer.current);timer.current=setTimeout(()=>{setChecking(false);setFailed(true);},15000);}
 const refreshLatest=useRef(refresh);refreshLatest.current=refresh;
 useEffect(()=>{if(wasActive.current&&!active)away.current=true;wasActive.current=active;if(!active)return;const resume=()=>{if(document.hidden){away.current=true;return;}if(away.current&&!dirty.current&&Date.now()-lastLoad.current>1000){away.current=false;refreshLatest.current();}};resume();window.addEventListener('focus',resume);window.addEventListener('pageshow',resume);document.addEventListener('visibilitychange',resume);return()=>{window.removeEventListener('focus',resume);window.removeEventListener('pageshow',resume);document.removeEventListener('visibilitychange',resume);};},[active]);
 useEffect(()=>()=>{detach.current();clearTimeout(timer.current);},[]);
 function loaded(e:React.SyntheticEvent<HTMLIFrameElement>){if(e.currentTarget!==frame.current)return;clearTimeout(timer.current);setChecking(false);setFailed(false);lastLoad.current=Date.now();sizeBoardInputs(e.currentTarget);try{const doc=e.currentTarget.contentDocument,win=e.currentTarget.contentWindow;if(!doc||!win)return;const origin=new URL(src).origin;if(win.location.origin!==origin)return;
 const input=(event:Event)=>{const target=event.target as Element|null;if(target?.matches('input[type=search],#search,#filter'))return;if(event.isTrusted||doc.activeElement===target)dirty.current=true;};const click=(event:Event)=>{const target=event.target as Element|null;if(target?.closest?.('#reload')){event.preventDefault();event.stopImmediatePropagation();refreshLatest.current();}};
 doc.addEventListener('input',input);doc.addEventListener('change',input);doc.addEventListener('click',click,true);detach.current=()=>{doc.removeEventListener('input',input);doc.removeEventListener('change',input);doc.removeEventListener('click',click,true);};if(attempt)win.scrollTo(0,scroll.current);
 }catch{/* Do not read or inject a foreign board. Its outer refresh still works. */}}
 return <div className="board-web"><iframe key={attempt} ref={frame} src={url} title={t('看板')} onLoad={loaded} sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-downloads" referrerPolicy="no-referrer"/>{failed&&<p role="status" className="board-web-refresh-error">{t('看板刷新未完成，请重试。')}</p>}<button type="button" className="board-web-refresh" aria-label={t('刷新看板')} aria-busy={checking} disabled={checking} onClick={refresh}><RefreshCw size={18} className={checking?'v4-progress-spinner':undefined}/></button><a className="board-web-open" href={src} target="_blank" rel="noreferrer noopener" onClick={()=>{away.current=true;}}><ExternalLink size={16}/>{t('在浏览器打开')}</a></div>;
}
