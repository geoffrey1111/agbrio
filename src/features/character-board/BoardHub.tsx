import {ReviewBoardReader} from './ReviewBoardReader';
import {selectedBoard,selectBoard,subscribeBoardSelection} from './boardSelection';
import {useEffect,useRef,useState,type ReactNode} from 'react';
import {isTauri} from '@tauri-apps/api/core';
import {Plus,ChevronRight,ChevronLeft} from 'lucide-react';
import {t,useLanguage} from '../../i18n';
import {CopyAction} from '../workbench/NativeReaderActions';
import {publishedBoardApi,type PublishedBoard} from './publishedBoardApi';
import {PublishedBoardReader} from './PublishedBoardReader';
import {boardExamples,boardInstructions,exampleDocument} from './boardExamples';
import './board-hub.css';
export function BoardHub({active,legacy,legacyAvailable,sourceLoading=false,retrySource,api=publishedBoardApi}:{active:boolean;legacy?:ReactNode;legacyAvailable:boolean;sourceLoading?:boolean;retrySource?:()=>void;api?:typeof publishedBoardApi}){
 useLanguage();const[cliPath,setCliPath]=useState<string|null>(null);const[boards,setBoards]=useState<PublishedBoard[]>([]),[ready,setReady]=useState(false),[error,setError]=useState(false),[selected,setSelected]=useState(()=>new URLSearchParams(location.search).get('board')??selectedBoard()),[creating,setCreating]=useState(false),[example,setExample]=useState<string|null>(null),[folder,setFolder]=useState(''),[importing,setImporting]=useState(false),[importError,setImportError]=useState('');
 const creatingRef=useRef(creating);creatingRef.current=creating;const previousIds=useRef<Set<string>|null>(null);
 const reload=useRef<()=>void>(()=>{});
 useEffect(()=>{if(!active)return;let alive=true,running=false;const load=async()=>{if(running||document.hidden)return;running=true;try{const next=await api.list();if(alive){const added=previousIds.current?next.find(b=>!previousIds.current!.has(b.id)):undefined;previousIds.current=new Set(next.map(b=>b.id));setBoards(next);if(added&&creatingRef.current){setSelected(added.id);setCreating(false);setExample(null);}setReady(true);setError(false);}}catch{if(alive)setError(true);}finally{running=false;}};reload.current=()=>void load();void load();const timer=setInterval(load,5000);window.addEventListener('online',load);document.addEventListener('visibilitychange',load);return()=>{reload.current=()=>{};alive=false;clearInterval(timer);window.removeEventListener('online',load);document.removeEventListener('visibilitychange',load);};},[active,api]);
 useEffect(()=>{if(!active)return;let alive=true;void api.help().then(v=>{if(alive)setCliPath(v.boards.cliPath);}).catch(()=>{});return()=>{alive=false;};},[active,api]);
 useEffect(()=>subscribeBoardSelection(()=>setSelected(selectedBoard())),[]);
 const managedReview=boards.find(b=>b.kind==='REVIEW'&&b.replacesLegacy);const effectiveLegacyAvailable=(ready||error)&&legacyAvailable&&!managedReview;
 const selectedBoardInfo=boards.find(b=>b.id===selected)??(selected==='legacy'?managedReview:undefined)??(!effectiveLegacyAvailable?managedReview??boards[0]:undefined);
 const failedWithoutBoard=(error||Boolean(retrySource))&&!effectiveLegacyAvailable&&!boards.length;
 const showGuide=creating||(!failedWithoutBoard&&!sourceLoading&&!effectiveLegacyAvailable&&ready&&!boards.length);
 const item=boardExamples.find(e=>e.id===example);
 async function connect(){setImporting(true);setImportError('');try{const board=await api.publish(folder.trim());setBoards(await api.list());setSelected(board.id);setCreating(false);setExample(null);}catch(e){setImportError(t(String(e).includes('MANIFEST')?'请确认目录内有有效的 agbrio-board.json，并在 files 中列出 index.html。':'没有接入成功，请检查文件清单、类型和大小后重试。'));}finally{setImporting(false);}}
 return <div className="board-hub">
  {(effectiveLegacyAvailable||boards.length>0)&&<div className="board-hub-toolbar">
   {showGuide&&(effectiveLegacyAvailable||boards.length>0)?<button type="button" className="r2-icon" aria-label={t('返回看板')} onClick={()=>{setCreating(false);setExample(null);}}><ChevronLeft size={20}/></button>:null}
   {(effectiveLegacyAvailable||boards.length>0)&&!showGuide?<select aria-label={t('选择看板')} value={selectedBoardInfo?.id??'legacy'} onChange={e=>{setSelected(e.target.value);selectBoard(e.target.value);}}>{effectiveLegacyAvailable&&<option value="legacy">{t('已连接看板')}</option>}{boards.map(b=><option key={b.id} value={b.id}>{b.title}</option>)}</select>:<strong>{t('看板')}</strong>}
   {!showGuide&&<button type="button" className="board-hub-create" onClick={()=>setCreating(true)}><Plus size={18}/>{t('创建看板')}</button>}
  </div>}
  {failedWithoutBoard?<div className="board-guide"><p role="status">{t('暂时无法读取看板，请确认电脑和 Agbrio 在线。')}</p><button type="button" className="board-example-back" onClick={()=>{retrySource?.();reload.current();}}>{t('重试')}</button></div>:null}
  {showGuide?<div className="board-guide">
   <div className="board-guide-intro"><h2>{t('把项目进展带到手机')}</h2><p>{t('让电脑上的 Agent 整理页面，在这里查看。')}</p></div>
   {!item?<div className="board-example-list">{boardExamples.map(e=><button type="button" key={e.id} onClick={()=>setExample(e.id)}><span><strong>{t(e.title)}</strong><small>{t(e.description)}</small></span><span className="board-example-link">{t('查看示例')}<ChevronRight size={18}/></span></button>)}</div>:<>
    <button type="button" className="board-example-back" onClick={()=>setExample(null)}><ChevronLeft size={18}/>{t('全部示例')}</button>
    <iframe className="board-example-frame" title={t(item.title)+' · '+t('示例')} sandbox="" srcDoc={exampleDocument(item.id)}/>
   </>}
   <div className="board-guide-action"><CopyAction showLabel label={t('复制给 Agent 的指令')} text={boardInstructions(item?.request??boardExamples[0].request,cliPath)}/><p>{t('发给电脑上的 Agent，创建后这里会自动出现。')}</p><small>{t('使用现有手机连接，无需另配域名；电脑和 Agbrio 需要保持在线。')}</small></div>
   {isTauri()&&<details className="board-import"><summary>{t('已有看板目录？接入这里')}</summary><label>{t('看板目录')}<input value={folder} onChange={e=>setFolder(e.target.value)} placeholder={'C:\\Projects\\my-board'}/></label><p>{t('只读取 agbrio-board.json 明确列出的文件。')}</p><button type="button" disabled={!folder.trim()||importing} onClick={()=>void connect()}>{t(importing?'正在接入…':'接入看板')}</button>{importError&&<p role="alert">{importError}</p>}</details>}
  </div>:!effectiveLegacyAvailable&&!selectedBoardInfo&&!failedWithoutBoard?<p className="board-source-muted" role="status">{t(sourceLoading||!ready?'正在读取…':'还没有选择看板')}</p>:null}
  {effectiveLegacyAvailable&&<div className="board-hub-content" hidden={showGuide||Boolean(selectedBoardInfo)}>{legacy}</div>}
  {selectedBoardInfo&&<div className="board-hub-content" hidden={showGuide}>{selectedBoardInfo.kind==='REVIEW'?<ReviewBoardReader id={selectedBoardInfo.id} active={active&&!showGuide}/>:<PublishedBoardReader id={selectedBoardInfo.id} active={active&&!showGuide} api={api}/>}</div>}
 </div>;
}
