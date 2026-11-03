import {readModels} from '../workbench/readModelCache';
const event='agbrio:board-selection';
const key=()=>`agbrio.board.selection.v1:${readModels.presentationScope()??'unpaired'}`;
export function selectedBoard(){try{return localStorage.getItem(key())??'';}catch{return '';}}
export function selectBoard(id:string){try{localStorage.setItem(key(),id);}catch{}window.dispatchEvent(new Event(event));}
export function subscribeBoardSelection(fn:()=>void){window.addEventListener(event,fn);return()=>window.removeEventListener(event,fn);}
