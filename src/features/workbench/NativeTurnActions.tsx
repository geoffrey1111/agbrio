import {useEffect,useRef,useState} from 'react';
import {t} from '../../i18n';
import type {NativeTurnCard,WatchReply} from './watchChatApi';
export function NativeTurnActions({card,disabled,unavailable,replies,submit}:{card:NativeTurnCard;disabled:boolean;unavailable?:string;replies:WatchReply[];submit:(text:string)=>Promise<string|undefined>}){
 const [selected,setSelected]=useState<number|null>(null),[sent,setSent]=useState(false),[attemptId,setAttemptId]=useState<string>();const lock=useRef(false);
 const status=replies.find(r=>r.id===attemptId)?.status;
 useEffect(()=>{if(status==='CANCELLED'||status==='FAILED'){lock.current=false;setSent(false);setAttemptId(undefined);}},[status]);
 if(!card.question&&!card.actions.length)return null;
 return <section className="v4-question-card" aria-label={t('下一步选择')}><fieldset disabled={disabled||Boolean(unavailable)||sent}><legend>{card.question}</legend><div className="v4-question-choices">{card.actions.map((a,i)=><label key={i} data-selected={selected===i}><input type="radio" name={card.id} checked={selected===i} onChange={()=>setSelected(i)}/><span>{a.label}</span></label>)}</div></fieldset>{unavailable?<p className="v4-meta" role="status">{t(unavailable)}</p>:card.actions.length>0&&<footer><span/><button type="button" className="v3-primary" disabled={disabled||sent||selected===null} onClick={()=>{if(lock.current||selected===null)return;lock.current=true;setSent(true);void submit(card.actions[selected].message).then(id=>{if(id)setAttemptId(id);else{lock.current=false;setSent(false);}});}}>{t(sent?'已提交，请查看发送状态':'发送选择')}</button></footer>}</section>;
}
