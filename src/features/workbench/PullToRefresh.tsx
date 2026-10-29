import {useEffect,useRef,useState,type ReactNode,type RefObject} from 'react';
import {ArrowDown,RefreshCw,Check} from 'lucide-react';import {t} from '../../i18n';
const THRESHOLD=56;
/** Read-only list gesture. Native scrolling stays passive; no page reload or draft reset. */
export function PullToRefresh({enabled,onRefresh,scrollContainer,children}:{enabled:boolean;onRefresh:()=>Promise<unknown>;scrollContainer?:RefObject<HTMLElement|null>;children:ReactNode}){
 const root=useRef<HTMLDivElement>(null),latest=useRef(onRefresh);latest.current=onRefresh;
 const pending=useRef(false),mounted=useRef(true),clickUntil=useRef(0),timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
 const [distance,setDistance]=useState(0),[phase,setPhase]=useState<'idle'|'busy'|'done'|'error'>('idle');
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;clearTimeout(timer.current);};},[]);
 async function refresh(){if(!enabled||pending.current)return;pending.current=true;clearTimeout(timer.current);setDistance(0);setPhase('busy');try{await latest.current();if(mounted.current){setPhase('done');timer.current=setTimeout(()=>setPhase('idle'),1200);}}catch{if(mounted.current)setPhase('error');}finally{pending.current=false;}}
 const trigger=useRef(refresh);trigger.current=refresh;
 useEffect(()=>{const node=scrollContainer?.current??root.current;if(!enabled){setDistance(0);return;}if(!node)return;let start:{x:number;y:number;id:number}|null=null,pull=0;
  const top=()=>Math.max(0,scrollContainer?.current?.scrollTop??node.closest<HTMLElement>('.r2-content')?.scrollTop??0)<=1;
  const reset=()=>{start=null;pull=0;setDistance(0);};
  const begin=(e:TouchEvent)=>{if(pending.current||!top()||e.touches.length!==1||(e.target as Element)?.closest?.('input,textarea,select,[contenteditable=true],.v6-chips'))return;const p=e.touches[0];start={x:p.clientX,y:p.clientY,id:p.identifier};pull=0;};
  const move=(e:TouchEvent)=>{if(!start)return;if(e.touches.length!==1||!top()){reset();return;}const p=e.touches[0];if(p.identifier!==start.id){reset();return;}const x=Math.abs(p.clientX-start.x),y=p.clientY-start.y;if(y<0||x>12&&x>Math.abs(y)){reset();return;}if(y>8&&y>x*1.2){pull=Math.min(82,y*.5);setDistance(pull);}};
  const end=()=>{const ready=pull>=THRESHOLD;if(pull>8)clickUntil.current=Date.now()+400;reset();if(ready)void trigger.current();};
  node.addEventListener('touchstart',begin,{passive:true});node.addEventListener('touchmove',move,{passive:true});node.addEventListener('touchend',end,{passive:true});node.addEventListener('touchcancel',reset,{passive:true});
  return()=>{node.removeEventListener('touchstart',begin);node.removeEventListener('touchmove',move);node.removeEventListener('touchend',end);node.removeEventListener('touchcancel',reset);};
 },[enabled,scrollContainer]);
 if(!enabled)return <>{children}</>;
 const shown=distance>0||phase!=='idle',label=phase==='busy'?t('正在刷新列表…'):phase==='done'?t('列表已更新'):phase==='error'?t('刷新未完成，当前列表已保留。'):distance>=THRESHOLD?t('松开刷新'):t('下拉刷新');
 return <div ref={root} className="list-pull-refresh" aria-busy={phase==='busy'} onClickCapture={e=>{if((e.target as Element)?.closest?.('.v6-row')&&e.detail>0&&Date.now()<clickUntil.current&&(!('pointerType' in e.nativeEvent)||e.nativeEvent.pointerType!=='mouse')){e.preventDefault();e.stopPropagation();}}}>
  <div className="list-pull-indicator" style={{height:shown?(distance||40):0}} aria-hidden={!shown}><span role="status" aria-live="polite">{phase==='busy'?<RefreshCw size={16} className="list-pull-spinner"/>:phase==='done'?<Check size={16}/>:<ArrowDown size={16} style={{transform:distance>=THRESHOLD?'rotate(180deg)':undefined}}/>}{label}{phase==='error'&&<button type="button" onClick={()=>void refresh()}>{t('重试')}</button>}</span></div>
  <button type="button" className="list-pull-accessible" disabled={pending.current} onClick={()=>void refresh()}>{t('刷新对话列表')}</button>{children}
 </div>;
}
