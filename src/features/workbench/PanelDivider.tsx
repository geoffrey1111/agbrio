import {useRef,useState} from "react";
export function PanelDivider({value,onChange,min=180,max=420,factor=1,step=16,resetValue,label="调整列表宽度"}:{value:number;onChange:(v:number)=>void;min?:number;max?:number;factor?:number;step?:number;resetValue?:number;label?:string}){
 const origin=useRef({x:0,value}),pointer=useRef<number|null>(null);const[dragging,setDragging]=useState(false);
 const clamp=(v:number)=>Math.max(min,Math.min(max,v));
 function finish(target:HTMLDivElement,cancel:boolean){const id=pointer.current;if(id===null)return;pointer.current=null;if(cancel)onChange(clamp(origin.current.value));if(target.hasPointerCapture(id))target.releasePointerCapture(id);setDragging(false);}
 return <div className="v5-panel-divider" data-dragging={dragging} role="separator" aria-label={label} aria-orientation="vertical" aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} tabIndex={0}
  onKeyDown={e=>{if(e.key==='Escape'&&pointer.current!==null){e.preventDefault();e.stopPropagation();finish(e.currentTarget,true);}else if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();onChange(clamp(e.key==='Home'?min:e.key==='End'?max:value+(e.key==='ArrowLeft'?-step:step)));}}}
  onDoubleClick={()=>onChange(clamp(resetValue??(min+max)/2))}
  onPointerDown={e=>{if(e.button!==0||pointer.current!==null)return;origin.current={x:e.clientX,value};pointer.current=e.pointerId;e.currentTarget.focus();e.currentTarget.setPointerCapture(e.pointerId);setDragging(true);}}
  onPointerMove={e=>{if(pointer.current===e.pointerId&&e.currentTarget.hasPointerCapture(e.pointerId))onChange(clamp(origin.current.value+(e.clientX-origin.current.x)/(Number.isFinite(factor)&&factor>0?factor:1)));}}
  onPointerUp={e=>{if(pointer.current===e.pointerId)finish(e.currentTarget,false);}}
  onPointerCancel={e=>{if(pointer.current===e.pointerId)finish(e.currentTarget,true);}} onLostPointerCapture={e=>{if(pointer.current===e.pointerId)finish(e.currentTarget,true);}}/>;
}
