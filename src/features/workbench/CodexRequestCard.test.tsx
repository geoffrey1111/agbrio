import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {CodexRequestCard} from './CodexRequestCard';
import type {MobileCodexRequest} from '../../mobile/api';
beforeEach(()=>sessionStorage.clear());afterEach(cleanup);
const request:MobileCodexRequest={requestId:'live-exact',revision:4,method:'item/tool/requestUserInput',kind:'USER_INPUT',isBlocking:false,responseSent:false,questions:[{id:'q1',label:'Scope',placeholder:'Choose the demo scope?',required:true,isOther:true,options:[{label:'Small',description:'Keep the change bounded'},{label:'Large',description:'Expand the change'}]},{id:'q2',label:'Notes',placeholder:'What should be retained?',required:true}]};
it('retains native choices, free text, multi-question navigation, collapse and one exact submission',async()=>{
 const respond=vi.fn(async()=>{});render(<CodexRequestCard request={request} disabled={false} respond={respond}/>);
 fireEvent.click(screen.getByRole('radio',{name:'Small Keep the change bounded'}));fireEvent.click(screen.getByRole('button',{name:'收起问题'}));expect(respond).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'回答问题'}));expect(screen.getByRole('radio',{name:'Small Keep the change bounded'})).toBeChecked();
 fireEvent.click(screen.getByRole('button',{name:'下一题'}));fireEvent.change(screen.getByLabelText('回答Notes'),{target:{value:'Keep the source paragraph.'}});
 fireEvent.click(screen.getByRole('button',{name:'发送'}));await waitFor(()=>expect(respond).toHaveBeenCalledWith({revision:4,answers:{q1:'Small',q2:'Keep the source paragraph.'}}));
});
it('explicit skip sends no invented answers and double clicks cannot respond twice',async()=>{
 let finish!:()=>void;const respond=vi.fn(()=>new Promise<void>(r=>{finish=r;}));render(<CodexRequestCard request={request} disabled={false} respond={respond}/>);
 const button=screen.getByRole('button',{name:'跳过'});fireEvent.click(button);fireEvent.click(button);
 expect(respond).toHaveBeenCalledTimes(1);expect(respond).toHaveBeenCalledWith({revision:4,decision:'skip'});finish();
});
it('secret answers stay out of session storage',()=>{
 render(<CodexRequestCard request={{...request,questions:[{id:'secret',label:'Secret',isSecret:true,required:true}]}} disabled={false} respond={vi.fn()}/>);
 const password=document.querySelector('input[type=password]')!;fireEvent.change(password,{target:{value:'fixture-only-secret'}});expect(sessionStorage.getItem('aiwr-request-answer:live-exact:4')).toBeNull();
});
