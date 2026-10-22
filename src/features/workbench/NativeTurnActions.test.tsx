import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {NativeTurnActions} from './NativeTurnActions';
import type {WatchReply} from './watchChatApi';
afterEach(cleanup);
const card={id:'exact-card',question:'选择下一步',actions:[{label:'方案 A',message:'执行 A'}]};
const receipt=(status:WatchReply['status'])=>({id:'exact-attempt',status}) as WatchReply;
it('allows another explicit selection after its exact queued attempt was cancelled, without automatic resubmission',async()=>{
 const submit=vi.fn(async()=>'exact-attempt');const view=render(<NativeTurnActions card={card} disabled={false} replies={[]} submit={submit}/>);
 fireEvent.click(screen.getByRole('radio'));fireEvent.click(screen.getByRole('button',{name:'发送选择'}));await waitFor(()=>expect(submit).toHaveBeenCalledTimes(1));
 view.rerender(<NativeTurnActions card={card} disabled replies={[receipt('QUEUED')]} submit={submit}/>);
 expect(screen.getByRole('radio')).toBeDisabled();
 view.rerender(<NativeTurnActions card={card} disabled={false} replies={[receipt('CANCELLED')]} submit={submit}/>);
 await waitFor(()=>expect(screen.getByRole('radio')).toBeEnabled());expect(screen.getByRole('button',{name:'发送选择'})).toBeEnabled();expect(submit).toHaveBeenCalledTimes(1);
});
it('keeps uncertain and sent attempts locked, and an unrelated failed attempt cannot unlock the card',async()=>{
 const submit=vi.fn(async()=>'exact-attempt');const view=render(<NativeTurnActions card={card} disabled={false} replies={[]} submit={submit}/>);
 fireEvent.click(screen.getByRole('radio'));fireEvent.click(screen.getByRole('button',{name:'发送选择'}));await waitFor(()=>expect(submit).toHaveBeenCalledTimes(1));
 for(const status of ['UNKNOWN','SENT'] as const){view.rerender(<NativeTurnActions card={card} disabled={false} replies={[receipt(status),{...receipt('FAILED'),id:'other'}]} submit={submit}/>);expect(screen.getByRole('radio')).toBeDisabled();}
 expect(submit).toHaveBeenCalledTimes(1);
});
it('unlocks a choice when the caller proved no command was accepted',async()=>{
 const submit=vi.fn(async()=>undefined);render(<NativeTurnActions card={card} disabled={false} replies={[]} submit={submit}/>);
 fireEvent.click(screen.getByRole('radio'));fireEvent.click(screen.getByRole('button',{name:'发送选择'}));await waitFor(()=>expect(submit).toHaveBeenCalledTimes(1));await waitFor(()=>expect(screen.getByRole('button',{name:'发送选择'})).toBeEnabled());
});
