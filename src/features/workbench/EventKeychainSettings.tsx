import {useEffect,useRef,useState} from 'react';
import {t,useLanguage} from '../../i18n';
export type EventKeychainStatus={supported:boolean;storedReferences:number;pendingCleanup:number;needsOwnerAction:boolean};
export type EventKeychainApi={status():Promise<EventKeychainStatus>;authorize(after:string|null):Promise<{checked:number;nextCursor:string|null;notificationSent:false}>};
export function EventKeychainSettings({api}:{api:EventKeychainApi}){
 useLanguage();const[view,setView]=useState<EventKeychainStatus|null>(null),[busy,setBusy]=useState(false),[cursor,setCursor]=useState<string|null>(null),[message,setMessage]=useState(''),[error,setError]=useState('');const pending=useRef(false),mounted=useRef(true);
 useEffect(()=>{mounted.current=true;void api.status().then(v=>{if(mounted.current)setView(v);}).catch(()=>{if(mounted.current)setError(t('事件钥匙串状态暂时不可用，请稍后重试。'));});return()=>{mounted.current=false;};},[api]);
 async function authorize(){if(pending.current)return;pending.current=true;setBusy(true);setError('');setMessage('');try{
  const result=await api.authorize(cursor);if(!mounted.current)return;
  setCursor(result.nextCursor);setMessage(t(result.nextCursor?'本批访问检查已完成。请继续检查下一批；没有发送测试通知。':'本批访问检查已完成；没有发送测试通知。'));setView(await api.status());
 }catch(e){if(mounted.current)setError(t(String(e).includes('KEYCHAIN_OWNER_ACTION_REQUIRED')?'macOS 需要本人授权或解锁。请完成系统提示后再试。':'事件钥匙串操作未完成，请检查系统状态后重试。'));}finally{pending.current=false;if(mounted.current)setBusy(false);}}
 if(!view?.supported)return error?<p role="alert">{error}</p>:null;
 return <section className="r2-settings-group" aria-label={t('MCP 事件钥匙串')}><h3>{t('MCP 事件钥匙串')}</h3><p>{t('事件密钥保存在当前用户的登录钥匙串。升级后系统可能需要本人重新授权，后台不会弹出提示。')}</p>
 {view.needsOwnerAction&&<p role="status">{t('macOS 需要本人授权或解锁。请完成系统提示后再试。')}</p>}
 <p>{t('请本人在系统提示中选择「始终允许」。此操作不创建订阅，已有通知沿用原有发送规则。')}</p>
 {view.pendingCleanup>0&&<p>{t('仍有 {0} 项旧密钥等待后台核对或清理。请保持钥匙串可用。',view.pendingCleanup)}</p>}
 <button type="button" className="r2-action" disabled={busy} onClick={()=>void authorize()}>{t(busy?'正在等待钥匙串…':cursor?'继续检查下一批':'授权读取事件密钥')}</button>
 {error&&<p role="alert">{error}</p>}{message&&<p role="status">{message}</p>}
 </section>;
}
