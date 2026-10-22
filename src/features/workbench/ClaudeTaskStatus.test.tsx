import {expect,it} from 'vitest';
import {watchTone} from './ConversationHome';
import type {CodexWatch} from './CodexNotifications';
const watch:CodexWatch={threadId:'claude_code:10000000-0000-4000-8000-000000000001',label:'manager',cwd:'D:/fictional',enabled:true,generation:1,checkedAt:Date.now(),errorCode:null,snapshot:{state:'RESULT_READY',turnId:'parent',itemId:'result',text:'intermediate parent report'},taskTree:{nodes:[],running:1,attention:0,unknown:0,limited:false}};
it('a manager with working descendants stays in the running directory filter after its own turn ends',()=>{
 expect(watchTone(watch)[0]).toBe('run');
 expect(watchTone({...watch,taskTree:{...watch.taskTree!,running:0,unknown:1}})[0]).toBe('done');
 expect(watchTone({...watch,enabled:false})[0]).toBe('done');
});
it('parent approval/failure and child attention remain visible while another child runs',()=>{
 for(const state of ['ACTION_REQUIRED','FAILED'])expect(watchTone({...watch,snapshot:{...watch.snapshot,state}})[0]).toBe('wait');
 expect(watchTone({...watch,taskTree:{...watch.taskTree!,attention:1}})[0]).toBe('wait');
});
