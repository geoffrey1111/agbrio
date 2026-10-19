import {convertFileSrc,invoke, isTauri} from '@tauri-apps/api/core';
import {useEffect, useMemo, useRef, useState} from 'react';
import {RefreshCw} from 'lucide-react';
import {liveRequest} from '../../mobile/api';
import {readModels} from '../workbench/readModelCache';
import {t as uiText} from '../../i18n';
import './character-board.css';
import {boardCache,verifyBoardAsset} from './boardCache';
import {boardDocument} from './boardDocument';
import {validReviewRequest,type ReviewRequest,type ReviewResponse} from './reviewProtocol';

type Reference={source:string;id:string};
export type BoardSnapshot={revision:string;html:string|null;assets:Reference[]};
export type BoardAsset={mime:string;data:string;filename:string;assetId:string;contentHash:string;sourceHash:string};
type Variant='original'|'preview'|'thumbnail';
type BoardSummary={revision:string;changed:boolean;assets:Reference[]};
const KEY='board:character';
export const boardApi={
  snapshot:async(knownRevision?:string,signal?:AbortSignal):Promise<BoardSnapshot>=>{
    if(isTauri())return invoke<BoardSnapshot>('character_board_snapshot',{knownRevision});
    const value=await liveRequest<BoardSnapshot|BoardSummary>(`/character-board${knownRevision?`?metadataOnly=true&knownRevision=${encodeURIComponent(knownRevision)}`:''}`,{signal});
    return 'changed' in value?(value.changed?liveRequest<BoardSnapshot>('/character-board',{signal}):{revision:value.revision,assets:[],html:null}):value;
  },
  review:(revision:string,input:ReviewRequest)=>isTauri()?invoke<ReviewResponse>('character_board_review',{revision,input}):liveRequest<ReviewResponse>('/character-board/review',{method:'POST',body:JSON.stringify({...input,revision})}),
  asset:(revision:string,assetId:string,variant:Variant='original',signal?:AbortSignal)=>isTauri()?invoke<BoardAsset>('character_board_asset',{revision,assetId,variant}):liveRequest<BoardAsset>(`/character-board/assets/${encodeURIComponent(assetId)}?revision=${encodeURIComponent(revision)}&variant=${variant}`,{signal}),
};
function message(error:unknown){return /401|403/.test(String(error))?uiText('连接已失效，请重新登录。'):uiText('暂时无法同步，稍后会自动重试。');}

/** Isolated original page; only catalog reads and the explicitly approved review vocabulary cross the boundary. */
export function CharacterBoard({active}:{active:boolean}){
  const [snapshot,setSnapshot]=useState(()=>readModels.peek<BoardSnapshot>(KEY));
  const [error,setError]=useState('');const [checking,setChecking]=useState(false);
  const [notice,setNotice]=useState('');const [verifiedRevision,setVerifiedRevision]=useState<string|null>(null);
  const reviewRevision=useRef<string|null>(null);reviewRevision.current=verifiedRevision;
  useEffect(()=>{if(!notice)return;const timer=setTimeout(()=>setNotice(''),5000);return()=>clearTimeout(timer);},[notice]);
  const [frameAttempt,setFrameAttempt]=useState(0);const frameReady=useRef(false),frameStarted=useRef(0);
  const frame=useRef<HTMLIFrameElement>(null),viewState=useRef<unknown>(null),visited=useRef(false);
  const media=useRef(new Map<string,BoardAsset>());
  const refreshRunning=useRef(false),loadVersion=useRef(0);
  const channel=useMemo(()=>crypto.randomUUID(),[snapshot?.revision,frameAttempt]);
  // Desktop keeps its independently CSP-protected document protocol; srcdoc would inherit the native shell's stricter script policy.
  const nativeUrl=useMemo(()=>snapshot&&isTauri()?convertFileSrc('view','character-board')+`?revision=${encodeURIComponent(snapshot.revision)}&channel=${channel}`:undefined,[snapshot?.revision,channel]);
  const document=useMemo(()=>!isTauri()&&snapshot?.html?boardDocument(snapshot.html,frameAttempt):undefined,[snapshot?.html,frameAttempt]);
  async function load(force=false){
    const version=++loadVersion.current,generation=readModels.generation(),cacheEpoch=boardCache.generation(),controller=new AbortController();let timer:ReturnType<typeof setTimeout>|undefined;
    try{
      const old=readModels.peek<BoardSnapshot>(KEY),next=await Promise.race([boardApi.snapshot(force?undefined:old?.revision,controller.signal),new Promise<never>((_,reject)=>{timer=setTimeout(()=>{controller.abort();reject(new Error('BOARD_SNAPSHOT_TIMEOUT'));},15000);})]);
      if(!next.html&&!old)throw new Error('BOARD_SNAPSHOT_MISSING');
      if(version===loadVersion.current&&generation===readModels.generation()){
        if(force){media.current.clear();frameStarted.current=Date.now();setFrameAttempt(n=>n+1);setNotice(uiText('已重新读取最新看板。'));}
        else if(old&&!frameReady.current&&frameStarted.current&&Date.now()-frameStarted.current>15000){frameStarted.current=Date.now();setFrameAttempt(n=>n+1);}
        setVerifiedRevision(next.revision);
        if(next.html&&cacheEpoch===boardCache.generation())void boardCache.saveSnapshot(next);
        setError('');
      }
      return next.html?next:old!;
    }catch(e){if(version===loadVersion.current)setError(message(e));throw e;}finally{clearTimeout(timer);}
  }
  const loader=useRef(load);loader.current=load;
  useEffect(()=>{
    if(!active&&!visited.current)return;
    visited.current=true;
    let disposed=false;const generation=readModels.generation();
    const unsubscribe=readModels.subscribe(KEY,()=>setSnapshot(readModels.peek(KEY)));
    void (async()=>{
      let hydrated=false;
      if(!readModels.peek(KEY)){
        const cached=await boardCache.snapshot();
        if(disposed||generation!==readModels.generation())return;
        if(cached){readModels.put(KEY,cached);setSnapshot(cached);setNotice(uiText('显示上次保存的看板，正在同步…'));hydrated=true;}
      }
      if(disposed)return;
      const pending=hydrated?readModels.refresh(KEY,()=>loader.current(),2000):readModels.read(KEY,()=>loader.current(),2000);
      await pending.then(value=>{if(!disposed)setSnapshot(value);}).catch(()=>undefined);
    })();
    return()=>{disposed=true;unsubscribe();};
  },[active]);
  useEffect(()=>{
    if(!snapshot)return;
    frameReady.current=false;frameStarted.current=Date.now();
    let disposed=false,running=0,scope='';const queue:{scope:string;run:()=>Promise<void>}[]=[];const controllers=new Set<AbortController>();
    const allowed=new Set(snapshot.assets.map(a=>a.id));const requests=new Set<string>(),reviewRequests=new Set<string>();
    const drain=()=>{while(!disposed&&running<3&&queue.length){running++;const index=queue.findIndex(q=>q.scope===scope);void queue.splice(index<0?0:index,1)[0].run().finally(()=>{running--;drain();});}};
    const listener=(event:MessageEvent)=>{
      if(event.source!==frame.current?.contentWindow||event.origin!=='null')return;
      if(event.data?.channel!==channel&&!(event.data?.kind==='ready'&&!event.data?.channel))return;
      const data=event.data;
      if(['ready','state','asset'].includes(data.kind))frameReady.current=true;
      const reply=(value:object)=>{if(!disposed)frame.current?.contentWindow?.postMessage({channel,...value},'*');};
      if(data.kind==='refresh'){void refresh();}
      else if(data.kind==='view'&&typeof data.scope==='string'){scope=data.scope.slice(0,100);drain();}
      else if(data.kind==='ready'){frameReady.current=true;reply({kind:'init',assets:snapshot.assets,state:viewState.current,reviewReady:reviewRevision.current===snapshot.revision});}
      else if(data.kind==='review'&&typeof data.request==='string'&&data.request.length<=12&&!reviewRequests.has(data.request)&&reviewRequests.size<1024){
        reviewRequests.add(data.request);
        if(!validReviewRequest(data.input)){reply({kind:'reviewResult',request:data.request,response:{status:403,json:{error:'看板不允许这个操作。'}}});return;}
        if(reviewRevision.current!==snapshot.revision){reply({kind:'reviewResult',request:data.request,response:{status:409,json:{error:'请等待最新看板同步完成后再批复。'}}});return;}
        // No retry, persistent queue, token storage, or optimistic success for writes.
        void boardApi.review(snapshot.revision,data.input).then(response=>reply({kind:'reviewResult',request:data.request,response})).catch(()=>reply({kind:'reviewResult',request:data.request,response:{status:503,json:{error:'连接中断；请核对已保存状态，不要重复提交。'}}}));
      }
      else if(data.kind==='state'){
        if(JSON.stringify(data.state??{}).length<20000)viewState.current=data.state;
      }else if(data.kind==='asset'&&typeof data.request==='string'&&data.request.length<=12&&allowed.has(data.id)&&!requests.has(data.request)&&requests.size<1024){
        if(data.variant!==undefined&&!['original','preview','thumbnail'].includes(data.variant))return;
        const variant:Variant=data.variant??'original',key=`${data.id}:${variant}`;
        requests.add(data.request);queue.push({scope:typeof data.scope==='string'?data.scope:'',run:async()=>{
          reply({kind:'assetStarted',request:data.request});
          const controller=new AbortController(),generation=readModels.generation(),cacheEpoch=boardCache.generation();controllers.add(controller);let timeout:ReturnType<typeof setTimeout>|undefined;
          try{
            let asset=media.current.get(key)??(variant==='original'?undefined:await boardCache.asset(data.id,variant))??undefined;
            if(!asset){
              asset=await Promise.race([boardApi.asset(snapshot.revision,data.id,variant,controller.signal).then(a=>verifyBoardAsset(a,data.id)),new Promise<never>((_,reject)=>{timeout=setTimeout(()=>{controller.abort();reject(new Error('BOARD_IMAGE_TIMEOUT'));},45000);})]);
              while(media.current.size>=96||[...media.current.values()].reduce((n,a)=>n+a.data.length,0)+asset.data.length>24*1024*1024){const oldest=media.current.keys().next().value;if(!oldest)break;media.current.delete(oldest);}
              if(disposed||generation!==readModels.generation()||cacheEpoch!==boardCache.generation())return;
              media.current.set(key,asset);void boardCache.saveAsset(asset,variant);
            }
            if(!disposed&&generation===readModels.generation()&&cacheEpoch===boardCache.generation())reply({kind:'assetResult',request:data.request,asset});
          }
          catch{reply({kind:'assetResult',request:data.request,error:uiText('这个文件暂时无法预览，可能已更新或移动。')});}
          finally{clearTimeout(timeout);controllers.delete(controller);}
        }});drain();
      }
    };
    window.addEventListener('message',listener);return()=>{disposed=true;queue.length=0;controllers.forEach(c=>c.abort());window.removeEventListener('message',listener);};
  },[snapshot?.revision,channel]);
  useEffect(()=>{if(snapshot)frame.current?.contentWindow?.postMessage({channel,kind:'reviewReady',ready:verifiedRevision===snapshot.revision},'*');},[verifiedRevision,snapshot?.revision,channel]);
  async function refresh(){if(refreshRunning.current)return;refreshRunning.current=true;setChecking(true);try{
    const pending=readModels.refresh(KEY,()=>load(true),2000,{supersede:true});
    // Manual reload is one-shot; later background checks must stay incremental.
    void readModels.read(KEY,()=>loader.current(),2000).catch(()=>undefined);
    setSnapshot(await pending);
  }catch{/* last good page stays visible */}finally{refreshRunning.current=false;setChecking(false);}}
  return <section className="character-board" aria-label={uiText('看板')}>
    <div className="character-board-toolbar"><span role="status">{checking?uiText('正在重新读取看板…'):error||notice||(snapshot?uiText('与电脑上的看板自动同步'):uiText('正在读取看板…'))}</span><button type="button" className="r2-icon" aria-label={uiText('刷新看板')} aria-busy={checking} disabled={checking} onClick={()=>void refresh()}><RefreshCw size={18} className={checking?'v4-progress-spinner':undefined}/></button></div>
    {!snapshot&&error&&<div className="r2-empty"><h2>{uiText('暂时无法打开看板')}</h2><p>{uiText('请确认电脑上的看板文件夹仍可访问，Agbrio 保持运行。')}</p></div>}
    {snapshot&&<iframe ref={frame} title={uiText('看板页面')} sandbox="allow-scripts" referrerPolicy="no-referrer" src={isTauri()?nativeUrl:undefined} srcDoc={isTauri()?undefined:document} data-board-channel={channel} data-board-revision={snapshot.revision} onLoad={()=>frame.current?.contentWindow?.postMessage({channel,kind:'init',assets:snapshot.assets,state:viewState.current,reviewReady:verifiedRevision===snapshot.revision},'*')}/>}
  </section>;
}
