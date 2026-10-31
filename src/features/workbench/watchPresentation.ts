import type {CodexWatch} from './CodexNotifications';
import {t} from '../../i18n';
export const QUIET_WATCH_AGE=3*24*60*60*1000;
export const threadProviderLabel=(id:string)=>id.startsWith('pi:')?'Pi':id.startsWith('claude_code:')?'Claude Code':'Codex';
export const threadAvatarKind=(id:string):'pi'|'claude'|'codex'=>id.startsWith('pi:')?'pi':id.startsWith('claude_code:')?'claude':'codex';
const timestamp=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)&&v>0?v:0;
export function isQuietWatch(w:CodexWatch,now=Date.now()):boolean{
 if(!w.enabled||w.errorCode||['RUNNING','ACTION_REQUIRED','FAILED'].includes(w.snapshot.state)||w.taskTree?.running||w.taskTree?.attention)return false;
 // Task-tree updatedAt is the native transcript's completed_at, not checkedAt.
 const childCompleted=Math.max(0,...(w.taskTree?.nodes?.map(n=>timestamp(n.updatedAt))??[]));
 const last=Math.max(timestamp(w.lastMessageAt),timestamp(w.shownAt),timestamp(w.changedAt),childCompleted);
 return last>0&&now-last>QUIET_WATCH_AGE;
}
export function watchStartError(error:unknown):string{
 const text=String(error);
 for(const [code,copy]of Object.entries({WATCH_LIMIT_REACHED:'监听数量已达到当前上限，请先暂停不需要的监听，再添加。',WATCH_ALREADY_IN_BRIDGE:'这条对话已在 Bridge 中，可直接从 Bridge 打开。',CLAUDE_SESSION_UNCONFIRMED:'这条 Claude Code 对话暂时不可读取。请在电脑的 Claude 中打开原对话，再刷新列表。',CLAUDE_SESSION_NOT_FOUND:'这条 Claude Code 对话暂时不可读取。请在电脑的 Claude 中打开原对话，再刷新列表。',CLAUDE_CODE_SESSION_NOT_FOUND:'这条 Claude Code 对话暂时不可读取。请在电脑的 Claude 中打开原对话，再刷新列表。',CLAUDE_ROOT_CHANGED:'Claude Code 对话位置已变化，请刷新列表后重新选择。',CLAUDE_TARGET_CHANGED:'Claude Code 对话位置已变化，请刷新列表后重新选择。'}))if(text.includes(code))return t(copy);
 if(text.includes('ROUTER_READ_TIMEOUT')||text.includes('Failed to fetch')||text.includes('NetworkError'))return t('没有确认监听是否成功，请检查电脑连接后刷新列表；不会自动重复添加。');
 return t('没有开始监听。请检查电脑端的连接状态，再刷新列表重试。');
}
