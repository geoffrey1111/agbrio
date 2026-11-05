import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {WatchChat} from './WatchChat';
import type {WatchChatApi,WatchChatState} from './watchChatApi';
beforeEach(()=>sessionStorage.clear());
afterEach(cleanup);
const original={sequence:17,threadId:'mac-keyboard-fixture',label:'Fixture',cwd:'/tmp/fixture',snapshot:{state:'RESULT_READY',turnId:'turn-a',itemId:'item-a',text:'Fixture result'},observedAt:1};
const state:WatchChatState={watch:{...original,enabled:true,generation:2,checkedAt:1,errorCode:null},host:'fixture',ownedTurnId:null,externalBusy:false,replies:[],requests:[],goal:null};
function fixture(){const command=vi.fn(async()=>({messages:[],nextCursor:null}));render(<WatchChat original={original} api={{state:async()=>state,command:command as WatchChatApi['command']}} onBack={()=>{}}/>);return command;}
it('Cmd+Enter sends exactly one idle reply through the existing native action',async()=>{
 const command=fixture();fireEvent.change(screen.getByRole('textbox'),{target:{value:'Mac reply'}});await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());
 fireEvent.keyDown(screen.getByRole('textbox'),{key:'Enter',metaKey:true});await waitFor(()=>expect(command.mock.calls.filter(args=>(args as unknown as [{action:string}])[0]?.action==='SEND')).toHaveLength(1));
});
it('Cmd+Enter during IME selection or a229 event never submits a partial candidate',async()=>{
 const command=fixture();const input=screen.getByRole('textbox');fireEvent.change(input,{target:{value:'还在选字'}});await waitFor(()=>expect(screen.getByRole('button',{name:'发送回复'})).toBeEnabled());
 fireEvent.compositionStart(input);fireEvent.keyDown(input,{key:'Enter',metaKey:true,isComposing:false});fireEvent.compositionEnd(input);fireEvent.keyDown(input,{key:'Enter',metaKey:true,keyCode:229});
 expect(command.mock.calls.filter(args=>(args as unknown as [{action:string}])[0]?.action==='SEND')).toHaveLength(0);
});
