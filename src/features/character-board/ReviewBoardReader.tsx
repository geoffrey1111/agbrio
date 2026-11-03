import {useEffect,useState} from 'react';
import {invoke,isTauri} from '@tauri-apps/api/core';import {t} from '../../i18n';import {UrlBoardFrame} from './UrlBoardFrame';
/** The explicitly owner-registered original review application; static boards stay opaque. */
export function ReviewBoardReader({id,active}:{id:string;active:boolean}){const[src,setSrc]=useState<string|null>(null),[error,setError]=useState(false);
 useEffect(()=>{let alive=true;setSrc(null);setError(false);const load=isTauri()?invoke<string>('review_board_desktop_url',{id}):Promise.resolve(new URL(`/v1/mobile/review-boards/${encodeURIComponent(id)}/frame/`,location.origin).href);void load.then(url=>{if(alive)setSrc(url)}).catch(()=>{if(alive)setError(true)});return()=>{alive=false}},[id]);
 return src?<UrlBoardFrame src={src} active={active} internalOnly/>:<p role="status">{t(error?'原审批服务暂不可用，请保持电脑上的看板服务运行。':'正在读取看板…')}</p>;
}
