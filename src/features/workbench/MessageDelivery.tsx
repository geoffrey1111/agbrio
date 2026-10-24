import {useState,type ReactNode} from 'react';
import {LoaderCircle,CircleAlert,Clock3,X} from 'lucide-react';
import * as Dialog from '@radix-ui/react-dialog';
import {t} from '../../i18n';
import './messageDelivery.css';
export function MessageDelivery({status,children}:{status:string;children?:ReactNode}){
 const [open,setOpen]=useState(false);
 if(['SENT','APPLIED','NATIVE'].includes(status))return null;
 const waiting=['SENDING','READY','OFFERED','PENDING'].includes(status);
 const label=waiting?t('正在发送'):status==='QUEUED'?t('已排队'):status==='FAILED'?t('未发送成功'):status==='CANCELLED'?t('已取消排队'):status==='ACKNOWLEDGED'?t('已检查，送达状态仍未确定'):t('送达尚未确认');
 if(waiting)return <span className="v4-delivery-indicator" role="status" aria-label={label}><LoaderCircle size={16} className="v4-delivery-spinner" aria-hidden="true"/><span className="sr-only">{label}</span></span>;
 return <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Trigger asChild><button type="button" className="v4-delivery-trigger" aria-label={label} title={label}>{status==='QUEUED'?<Clock3 size={16} aria-hidden="true"/>:<CircleAlert size={16} aria-hidden="true"/>}</button></Dialog.Trigger><Dialog.Portal><Dialog.Overlay className="v4-chat-sheet-overlay"/><div className="v4-chat-sheet-frame"><Dialog.Content className="v4-chat-sheet" aria-describedby={undefined}><header><Dialog.Title>{t('发送详情')}</Dialog.Title><Dialog.Close asChild><button type="button" aria-label={t('关闭{0}',t('发送详情'))}><X size={24} aria-hidden="true"/></button></Dialog.Close></header><div className="v4-chat-sheet-body"><p>{label}</p>{children}</div></Dialog.Content></div></Dialog.Portal></Dialog.Root>;
}
