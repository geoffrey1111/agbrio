import {invoke,isTauri} from '@tauri-apps/api/core';
import {liveRequest} from '../../mobile/api';
import type {BoardAsset} from './CharacterBoard';
export type PublishedBoard={id:string;title:string;revision:string;available:boolean};
export type PublishedSnapshot={board:PublishedBoard;html:string|null;assets:{path:string;id:string;mime:string}[];inline:Record<string,string>};
export const publishedBoardApi={
 help:()=>isTauri()?invoke<{boards:{cliPath:string|null}}>('app_capabilities'):liveRequest<{boards:{cliPath:string|null}}>('/app/capabilities'),
 list:()=>isTauri()?invoke<PublishedBoard[]>('published_boards_list'):liveRequest<PublishedBoard[]>('/boards'),
 snapshot:(id:string,knownRevision?:string)=>isTauri()?invoke<PublishedSnapshot>('published_board_snapshot',{id,knownRevision}):liveRequest<PublishedSnapshot>(`/boards/${encodeURIComponent(id)}${knownRevision?`?knownRevision=${encodeURIComponent(knownRevision)}`:''}`),
 asset:(id:string,revision:string,assetId:string)=>isTauri()?invoke<BoardAsset>('published_board_asset',{id,revision,assetId}):liveRequest<BoardAsset>(`/boards/${encodeURIComponent(id)}/assets/${encodeURIComponent(assetId)}?revision=${encodeURIComponent(revision)}`),
 publish:(folder:string)=>invoke<PublishedBoard>('published_board_publish',{folder}),
};
