import {invoke,isTauri} from '@tauri-apps/api/core';
import {useEffect,useState} from 'react';
import {ExternalLink,FolderOpen,Globe} from 'lucide-react';
import {liveRequest} from '../../mobile/api';
import {t as uiText,useLanguage} from '../../i18n';
import {CharacterBoard} from './CharacterBoard';
import {sizeBoardInputs} from './mobileInputSizing';

/** Where the board comes from. Only the desktop may change it. */
export type BoardSource={kind:'LOCAL';root:string;extraRoots?:string[]}|{kind:'URL';url:string;desktopUrl?:string|null};
export const boardSourceApi={
 read:():Promise<BoardSource|null>=>isTauri()?invoke<BoardSource|null>('board_source_read'):liveRequest<BoardSource|null>('/board-source'),
 write:(source:BoardSource):Promise<BoardSource>=>invoke<BoardSource>('board_source_write',{source}),
};
// undefined: not read yet; null: the owner has not chosen a board.
let current:BoardSource|null|undefined=undefined;const listeners=new Set<(s:BoardSource|null)=>void>();
function publish(next:BoardSource|null){current=next;for(const fn of listeners)fn(next);}
function useBoardSource(enabled=true){
 const[source,setSource]=useState<BoardSource|null|undefined>(current);
 useEffect(()=>{listeners.add(setSource);if(enabled&&current===undefined)void boardSourceApi.read().then(publish).catch(()=>undefined);return()=>{listeners.delete(setSource);};},[enabled]);
 return source;
}

/** A web board is embedded as-is; a local folder uses the existing isolated projection. */
export function BoardView({active}:{active:boolean}){
 useLanguage();const source=useBoardSource(active);
 if(source?.kind==='URL'){const src=isTauri()&&source.desktopUrl?source.desktopUrl:source.url;return <div className="board-web"><iframe src={src} title={uiText('看板')} onLoad={e=>sizeBoardInputs(e.currentTarget)} sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-downloads" referrerPolicy="no-referrer"/><a className="board-web-open" href={src} target="_blank" rel="noreferrer noopener"><ExternalLink size={16}/>{uiText('在浏览器打开')}</a></div>;}
 if(source===null)return <div className="v6-empty board-unset"><h2>{uiText('还没有选择看板')}</h2><p>{isTauri()?uiText('在 设置 → 看板来源 里选择一个本地文件夹或网页地址。'):uiText('在电脑上的 Agbrio：设置 → 看板来源 里选择。')}</p></div>;
 if(source===undefined)return <p className="board-source-muted" role="status">{uiText('正在读取…')}</p>;
 return <CharacterBoard active={active}/>;
}

export function BoardSourceSettings(){
 useLanguage();const source=useBoardSource();const editable=isTauri();
 const[kind,setKind]=useState<'LOCAL'|'URL'>('LOCAL'),[value,setValue]=useState(''),[desktopValue,setDesktopValue]=useState(''),[extraValue,setExtraValue]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false);
 useEffect(()=>{if(!source)return;setKind(source.kind);setValue(source.kind==='URL'?source.url:source.root);setDesktopValue(source.kind==='URL'?source.desktopUrl??'':'');setExtraValue(source.kind==='LOCAL'?(source.extraRoots??[]).join('\n'):'');},[source]);
 async function save(){
  setBusy(true);setError('');setSaved(false);
  try{publish(await boardSourceApi.write(kind==='URL'?{kind,url:value.trim(),desktopUrl:desktopValue.trim()||null}:{kind,root:value.trim(),extraRoots:extraValue.split(/\r?\n/).map(v=>v.trim()).filter(Boolean)}));setSaved(true);}
  catch(e){setError(String(e).includes('BOARD_URL_INVALID')?uiText('请填写 https 开头的网址。'):String(e).includes('BOARD_DESKTOP_URL_INVALID')?uiText('电脑地址只能是 https，或本机的 http://127.0.0.1 / localhost。'):String(e).includes('BOARD_EXTRA_FOLDER_INVALID')?uiText('额外图片文件夹需要是已存在的完整路径，最多 16 个。'):String(e).includes('BOARD_FOLDER_INVALID')?uiText('文件夹里需要有 index.html。请填写完整路径。'):uiText('没有保存成功，请重试。'));}
  finally{setBusy(false);}
 }
 if(source===undefined)return <p className="board-source-muted" role="status">{uiText('正在读取…')}</p>;
 return <form className="board-source" onSubmit={e=>{e.preventDefault();if(editable)void save();}}>
  <div className="board-source-seg" role="radiogroup" aria-label={uiText('看板来源')}>
   {(['LOCAL','URL'] as const).map(k=><button key={k} type="button" role="radio" aria-checked={kind===k} disabled={!editable||busy} onClick={()=>{setKind(k);setValue(source?.kind===k?(k==='URL'?(source as {url:string}).url:(source as {root:string}).root):'');setSaved(false);}}>{k==='LOCAL'?<FolderOpen size={16}/>:<Globe size={16}/>}{k==='LOCAL'?uiText('本地文件夹'):uiText('网页地址')}</button>)}
  </div>
  <label>{kind==='LOCAL'?uiText('看板文件夹（含 index.html）'):uiText('看板网址')}<input value={value} disabled={!editable||busy} placeholder={kind==='LOCAL'?'D:\\path\\to\\board':'https://board.example.com'} onChange={e=>{setValue(e.target.value);setSaved(false);}} spellCheck={false} autoComplete="off"/></label>
  {kind==='LOCAL'&&<label>{uiText('额外的图片文件夹（可选，每行一个）')}<textarea rows={3} value={extraValue} disabled={!editable||busy} placeholder={'D:\\art\\characters'} onChange={e=>{setExtraValue(e.target.value);setSaved(false);}} spellCheck={false}/></label>}
  {kind==='URL'&&<label>{uiText('电脑上使用的地址（可选）')}<input value={desktopValue} disabled={!editable||busy} placeholder="http://127.0.0.1:47816/board/" onChange={e=>{setDesktopValue(e.target.value);setSaved(false);}} spellCheck={false} autoComplete="off"/></label>}
  <p className="board-source-muted">{kind==='LOCAL'?uiText('Agbrio 只读取这个文件夹里的 index.html，以及它引用的、位于这个文件夹或上面列出的额外文件夹中的图片；手机通过电脑查看。'):uiText('看板页面会直接嵌入，登录和审批由网页自己负责。只有允许被 Agbrio 嵌入的网页才能显示；多数公开网站会拒绝嵌入。电脑上可以填本机地址，直接打开同一个看板。')}</p>
  {error&&<p role="alert" className="board-source-error">{error}</p>}
  {editable?<button type="submit" className="board-source-save" disabled={busy||!value.trim()}>{busy?uiText('正在保存…'):saved?uiText('已保存'):uiText('保存')}</button>:<p className="board-source-muted">{uiText('看板来源只能在电脑上的 Agbrio 设置里更改。')}</p>}
 </form>;
}
