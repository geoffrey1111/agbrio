import {useEffect,useRef,useState,type ReactNode,type RefObject} from 'react';
import {Bell,FolderOpen,Gauge,Monitor,Shield,SlidersHorizontal,Smartphone} from 'lucide-react';
import {t} from '../../i18n';

export const settingsGroups=[['OVERVIEW','概览',Gauge],['CONNECTIONS','连接',Monitor],['DEVICES','手机与设备',Smartphone],['NOTIFY','通知',Bell],['PROTECTION','运行保护',Shield],['WORKSPACE','工作区设置',FolderOpen],['GENERAL','应用维护',SlidersHorizontal]] as const;
export type SettingsGroup=typeof settingsGroups[number][0];
export const settingsAnchor=(id:SettingsGroup)=>`settings-${id.toLowerCase()}`;

export function useSettingsIndex(active:boolean,scroller:RefObject<HTMLElement|null>){
 const [current,setCurrent]=useState<SettingsGroup>('OVERVIEW');
 const [request,setRequest]=useState<{id:SettingsGroup;target?:string}|null>(null);
 const frame=useRef(0);
 const sync=()=>{const root=scroller.current;if(!root)return;const top=root.getBoundingClientRect().top;
  const groups=Array.from(root.querySelectorAll<HTMLElement>('[data-settings-group]'));
  const match=groups.filter(el=>el.getBoundingClientRect().top<=top+72).at(-1)??groups[0];
  const atBottom=root.scrollHeight>root.clientHeight&&root.scrollHeight-root.scrollTop-root.clientHeight<=2;
  const id=(atBottom?groups.at(-1):match)?.dataset.settingsGroup as SettingsGroup|undefined;
  if(id)setCurrent(id);
 };
 useEffect(()=>{const root=scroller.current;if(!active||!root)return;
  const page=root.querySelector<HTMLElement>('.settings-index');
  const update=()=>{cancelAnimationFrame(frame.current);frame.current=requestAnimationFrame(()=>{
   page?.style.setProperty('--settings-visible-height',`${root.clientHeight}px`);sync();
  });};
  root.addEventListener('scroll',update,{passive:true});const observer=typeof ResizeObserver==='function'?new ResizeObserver(update):null;observer?.observe(root);if(page)observer?.observe(page);update();
  return()=>{cancelAnimationFrame(frame.current);root.removeEventListener('scroll',update);observer?.disconnect();};
 },[active,scroller]);
 useEffect(()=>{if(!active||!request)return;const root=scroller.current;const section=root?.querySelector<HTMLElement>(`#${request.target??settingsAnchor(request.id)}`);if(!root||!section)return;
  root.scrollTop+=section.getBoundingClientRect().top-root.getBoundingClientRect().top-16;
  section.querySelector<HTMLElement>('h2,h3')?.focus({preventScroll:true});setCurrent(request.id);setRequest(null);
 },[active,request,scroller]);
 return {current,jump:(id:SettingsGroup,target?:string)=>setRequest({id,target})};
}

export function SettingsJumpNav({groups,current,jump,compact=false}:{groups:SettingsGroup[];current:SettingsGroup;jump:(id:SettingsGroup)=>void;compact?:boolean}){
 const selected=useRef<HTMLAnchorElement>(null),nav=useRef<HTMLElement>(null);
 useEffect(()=>{const root=nav.current,link=selected.current;if(!compact||!root||!link)return;const a=link.getBoundingClientRect(),b=root.getBoundingClientRect();if(a.left<b.left)root.scrollLeft+=a.left-b.left-8;else if(a.right>b.right)root.scrollLeft+=a.right-b.right+8;},[compact,current]);
 return <nav ref={nav} className={compact?'settings-jump-compact':'v7-settings-nav settings-jump'} aria-label={t('设置分类')}>
  {!compact&&<div className="settings-jump-title">{t('设置')}</div>}
  {settingsGroups.filter(([id])=>groups.includes(id)).map(([id,label,Icon])=><a key={id} ref={current===id?selected:undefined} href={`#${settingsAnchor(id)}`} aria-current={current===id?'location':undefined} onClick={e=>{e.preventDefault();jump(id);}}><Icon size={17} aria-hidden/><span>{t(label)}</span></a>)}
 </nav>;
}

export function SettingsIndexGroup({id,children}:{id:SettingsGroup;children:ReactNode}){
 const label=settingsGroups.find(([key])=>key===id)![1];
 return <section id={settingsAnchor(id)} data-settings-group={id} className="settings-index-group" aria-labelledby={`${settingsAnchor(id)}-title`}>
  <h2 id={`${settingsAnchor(id)}-title`} tabIndex={-1}>{t(label)}</h2>{children}
 </section>;
}

export function SettingsIndexItem({id,label,children}:{id?:string;label:string;children:ReactNode}){
 return <div id={id} className="settings-index-item"><h3 tabIndex={-1}>{t(label)}</h3>{children}</div>;
}
