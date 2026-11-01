import {useState,type CSSProperties} from 'react';
import {ChevronRight} from 'lucide-react';
import {t,tc,useLanguage} from '../../i18n';
import type {ChildEntry,ChildTreeModel} from './childTree';

const statusTone=(e:ChildEntry)=>e.tone==='wait'?'wait':e.tone==='fail'?'fail':e.tone==='run'?'run':'done';
/** Compact delegated-conversation tree. Same rows in the list and the detail
 * panel: 16px per level, indentation stops at the third level. */
export function ChildTreeList({model,onOpen,selectedKey,label,expandEnded=false}:{model:ChildTreeModel;onOpen:(entry:ChildEntry)=>void;selectedKey?:string;label?:string;expandEnded?:boolean}){
 useLanguage();
 const [endedOpen,setEndedOpen]=useState(expandEnded);
 const node=(e:ChildEntry)=><button type="button" key={`${e.kind}:${e.key}`} className="v7-tree-node" data-tone={e.tone} data-kind={e.kind} data-depth={Math.min(e.depth,3)} style={{'--v7-tree-depth':Math.min(Math.max(e.depth,1),3)-1} as CSSProperties} aria-current={selectedKey===e.key?'true':undefined} onClick={()=>onOpen(e)}>
  <span className={`v7-tree-badge v7-tree-${e.kind}`} aria-hidden>{e.kind==='pi'?'π':'✳'}</span>
  <span className="v7-tree-title" title={e.title}>{e.title}</span>
  {e.tone==='new'&&<i className="v7-result-dot" role="img" aria-label={t('有新结果')}/>}
  <span className="v7-status" data-tone={statusTone(e)}>{e.label}</span>
 </button>;
 return <div className="v7-tree" role="group" aria-label={label??t('子对话')}>
  {model.active.map(node)}
  {model.ended.length>0&&<><button type="button" className="v7-tree-ended" aria-expanded={endedOpen} onClick={()=>setEndedOpen(v=>!v)}><ChevronRight size={14} aria-hidden/><span>{tc('已结束 · {0} 项',model.ended.length)}{model.closed?t('（含已关闭 {0}）',model.closed):''}</span></button>{endedOpen&&model.ended.map(node)}</>}
  {(model.limited||model.unknown>0)&&<p className="v7-tree-note" role="status">{t('部分关系或状态暂时无法读取，未将它们视为完成。')}</p>}
 </div>;
}
