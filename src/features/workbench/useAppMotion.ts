import {useEffect} from "react";
/** One interaction vocabulary: compress on contact, spring back on release,
 * lift the selected navigation icon, and land a submitted message. Layout and
 * swipe transforms remain owned by Motion; GSAP touches their child icons only. */
export function useAppMotion(){
 useEffect(()=>{
  let cancelled=false,dispose=()=>{};
  void import("gsap").then(({gsap})=>{if(cancelled)return;const media=gsap.matchMedia();
  media.add('(prefers-reduced-motion: no-preference)',context=>{
   let pressed:SVGElement|null=null,lastSubmit=0;
   const owned=(node:Element)=>Boolean(node.closest('.r2-router,.v5-role-chat,.v4-chat-page,.v4-chat-sheet,.r2-sheet,.r2-sheet-frame,.v5-relay-review'));
   const icon=(event:Event)=>{const target=event.target;if(!(target instanceof Element)||!owned(target))return null;const button=target.closest<HTMLButtonElement>('button');if(!button||button.disabled)return null;return button.querySelector<SVGElement>('svg');};
   context.add('release',()=>{if(!pressed)return;gsap.to(pressed,{scale:1,y:0,duration:.32,ease:'back.out(1.6)',overwrite:'auto',clearProps:'transform,transformOrigin'});pressed=null;});
   context.add('press',(event:Event)=>{context.release();const node=icon(event);if(!node)return;pressed=node;gsap.to(node,{scale:.84,duration:.09,ease:'power3.out',transformOrigin:'50% 50%',overwrite:'auto'});});
   context.add('selected',(node:Element)=>{const svg=node.querySelector('svg');if(svg)gsap.fromTo(svg,{scale:.88,y:3},{scale:1,y:0,duration:.34,ease:'back.out(1.5)',overwrite:'auto',clearProps:'transform,transformOrigin'});});
   context.add('sent',(node:Element)=>{gsap.timeline().fromTo(node,{y:10,opacity:.8},{y:0,opacity:1,duration:.22,ease:'power3.out',clearProps:'transform,opacity'});});
   const submit=(event:Event)=>{if(event.target instanceof Element&&owned(event.target))lastSubmit=performance.now();};
   const keydown=(event:KeyboardEvent)=>{if(!event.repeat&&['Enter',' '].includes(event.key))context.press(event);};
   const keyup=(event:KeyboardEvent)=>{if(['Enter',' '].includes(event.key))context.release();};
   const observer=new MutationObserver(records=>{for(const record of records){
    if(record.type==='attributes'&&record.target instanceof Element&&record.target.matches('.r2-tabbar button[aria-current=page],.r2-destinations button[aria-current=page]'))context.selected(record.target);
    if(record.type==='childList'&&lastSubmit>0&&performance.now()-lastSubmit<2000)for(const node of record.addedNodes){if(node instanceof Element&&node.matches('.v4-chat-message[data-message-kind=receipt]')&&owned(node))context.sent(node);}
   }});
   observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['aria-current']});
   document.addEventListener('pointerdown',context.press,true);document.addEventListener('pointerup',context.release,true);document.addEventListener('pointercancel',context.release,true);
   document.addEventListener('keydown',keydown,true);document.addEventListener('keyup',keyup,true);document.addEventListener('submit',submit,true);
   return()=>{observer.disconnect();document.removeEventListener('pointerdown',context.press,true);document.removeEventListener('pointerup',context.release,true);document.removeEventListener('pointercancel',context.release,true);document.removeEventListener('keydown',keydown,true);document.removeEventListener('keyup',keyup,true);document.removeEventListener('submit',submit,true);pressed=null;};
  });
  dispose=()=>media.revert();}).catch(()=>{/* optional motion must not block app operation */});
  return()=>{cancelled=true;dispose();};
 },[]);
}
