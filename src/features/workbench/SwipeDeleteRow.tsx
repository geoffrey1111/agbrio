import {t as uiText,useLanguage} from "../../i18n";
import {useEffect,useId,useRef,useState,type ReactNode} from 'react';
import {Trash2} from 'lucide-react';

/** Both horizontal directions reveal Delete. Native Touch Events also cover
 * Safari's touch arbitration; only a locked horizontal gesture is cancelled. */
export function SwipeDeleteRow({label,onDelete,children,disabled=false,className=''}:{label:string;onDelete:()=>void;children:ReactNode;disabled?:boolean;className?:string}){
 useLanguage();
 const id=useId(),row=useRef<HTMLDivElement>(null),[offset,setOffset]=useState(0),[dragging,setDragging]=useState(false);
 const gesture=useRef<{x:number;y:number;start:number;axis:'pending'|'x'|'y';offset:number}|null>(null),suppress=useRef(0);
 function reveal(value:number){setOffset(value);if(value)window.dispatchEvent(new CustomEvent('agbrio-swipe-open',{detail:id}));}
 function start(x:number,y:number,target:EventTarget|null){if(disabled||x<20||x>window.innerWidth-20||(target as HTMLElement)?.closest('.agbrio-swipe-delete,.agbrio-row-delete'))return;gesture.current={x,y,start:offset,axis:'pending',offset};}
 function move(x:number,y:number){const g=gesture.current;if(!g)return false;const dx=x-g.x,dy=y-g.y;if(g.axis==='pending'&&Math.max(Math.abs(dx),Math.abs(dy))>9)g.axis=Math.abs(dx)>Math.abs(dy)*1.3?'x':'y';if(g.axis!=='x')return false;setDragging(true);g.offset=Math.max(-88,Math.min(88,g.start+dx));setOffset(g.offset);return true;}
 function finish(){const g=gesture.current;gesture.current=null;setDragging(false);if(g?.axis==='x'){suppress.current=performance.now()+400;reveal(Math.abs(g.offset)>=40?Math.sign(g.offset)*76:0);}}
 function cancel(){gesture.current=null;setDragging(false);setOffset(0);}
 useEffect(()=>{const other=(e:Event)=>{if((e as CustomEvent).detail!==id)setOffset(0);};const outside=(e:PointerEvent)=>{if(!row.current?.contains(e.target as Node))setOffset(0);};window.addEventListener('agbrio-swipe-open',other);window.addEventListener('pointerdown',outside);return()=>{window.removeEventListener('agbrio-swipe-open',other);window.removeEventListener('pointerdown',outside);};},[id]);
 // React's document touchmove listeners are passive. A row-local non-passive
 // listener is needed to retain a horizontal touch without blocking scroll.
 const touchHandlers=useRef({start,move,finish,cancel});touchHandlers.current={start,move,finish,cancel};
 useEffect(()=>{const element=row.current;if(!element||disabled)return;const down=(e:TouchEvent)=>{if(e.touches.length!==1){touchHandlers.current.cancel();return;}const t=e.touches[0];touchHandlers.current.start(t.clientX,t.clientY,e.target);};const update=(e:TouchEvent)=>{if(e.touches.length!==1){touchHandlers.current.cancel();return;}const t=e.touches[0];if(touchHandlers.current.move(t.clientX,t.clientY)&&e.cancelable)e.preventDefault();};const end=()=>touchHandlers.current.finish(),abort=()=>touchHandlers.current.cancel();element.addEventListener('touchstart',down,{passive:true});element.addEventListener('touchmove',update,{passive:false});element.addEventListener('touchend',end);element.addEventListener('touchcancel',abort);return()=>{element.removeEventListener('touchstart',down);element.removeEventListener('touchmove',update);element.removeEventListener('touchend',end);element.removeEventListener('touchcancel',abort);};},[disabled]);
 if(disabled)return <div className={`agbrio-swipe-row ${className}`}><div className="agbrio-swipe-content">{children}</div></div>;
 return <div ref={row} className={`agbrio-swipe-row ${className}`} data-open={Math.abs(offset)>=40} data-revealed={Math.abs(offset)>0} data-side={offset<0?'right':'left'} data-dragging={dragging}
  onPointerDown={e=>{if(e.pointerType==='mouse'||e.pointerType==='touch')return;start(e.clientX,e.clientY,e.target);}}
  onPointerMove={e=>{if(e.pointerType==='touch')return;if(move(e.clientX,e.clientY))e.currentTarget.setPointerCapture?.(e.pointerId);}}
  onPointerUp={e=>{if(e.pointerType!=='touch')finish();}}
  onPointerCancel={e=>{if(e.pointerType!=='touch')cancel();}}
  onKeyDown={e=>{if(e.key==='Escape'){reveal(0);e.stopPropagation();}}}
  onContextMenu={e=>{if(disabled)return;e.preventDefault();reveal(offset?0:76);}}
  onClickCapture={e=>{if(!(e.target as HTMLElement).closest('.agbrio-swipe-content'))return;if(performance.now()<suppress.current||Math.abs(offset)>=40){e.preventDefault();e.stopPropagation();if(performance.now()>=suppress.current)reveal(0);}}}>
  <button type="button" className="agbrio-swipe-delete" aria-label={uiText("删除 {0}", label)} aria-hidden={Math.abs(offset)<40} tabIndex={Math.abs(offset)>=40?0:-1} disabled={disabled} onClick={onDelete}><Trash2 size={20} aria-hidden="true"/><span>{uiText("删除")}</span></button>
  <div className="agbrio-swipe-content" style={{transform:`translateX(${offset}px)`}}>{children}</div>
  <button type="button" className="agbrio-row-delete" aria-label={uiText("删除 {0}", label)} disabled={disabled} onClick={onDelete}><Trash2 size={18} aria-hidden="true"/></button>
 </div>;
}
