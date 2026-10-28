import {expect,it} from 'vitest';
import {isQuietWatch,QUIET_WATCH_AGE,watchStartError} from './watchPresentation';
import type {CodexWatch} from './CodexNotifications';
const now=Date.now();
const old:CodexWatch={threadId:'exact',label:'old',cwd:'D:/fixture',enabled:true,generation:1,checkedAt:now,errorCode:null,lastMessageAt:now-QUIET_WATCH_AGE-1,snapshot:{state:'IDLE',turnId:null,itemId:null,text:''}};
it('uses message and explicit shown times, never a fresh poll timestamp',()=>{
 expect(isQuietWatch(old,now)).toBe(true);expect(isQuietWatch({...old,lastMessageAt:now-QUIET_WATCH_AGE},now)).toBe(false);
 expect(isQuietWatch({...old,shownAt:now-10},now)).toBe(false);expect(isQuietWatch({...old,lastMessageAt:now-10},now)).toBe(false);
 expect(isQuietWatch({...old,changedAt:now-10},now)).toBe(false);
 expect(isQuietWatch({...old,lastMessageAt:null},now)).toBe(false);
 expect(isQuietWatch({...old,taskTree:{nodes:[{updatedAt:now-10}],running:0,attention:0} as never},now)).toBe(false);
});
it('keeps execution, decisions, failures and unavailable reads in view',()=>{
 for(const state of ['RUNNING','ACTION_REQUIRED','FAILED'])expect(isQuietWatch({...old,snapshot:{...old.snapshot,state}},now)).toBe(false);
 expect(isQuietWatch({...old,errorCode:'READ_UNAVAILABLE'},now)).toBe(false);
 expect(isQuietWatch({...old,taskTree:{running:1,attention:0} as never},now)).toBe(false);
 expect(isQuietWatch({...old,taskTree:{running:0,attention:1} as never},now)).toBe(false);
});
it('explains a capacity rejection and distinguishes a lost acknowledgement',()=>{
 expect(watchStartError('WATCH_LIMIT_REACHED')).toContain('上限');expect(watchStartError('Failed to fetch')).toContain('没有确认');
});
