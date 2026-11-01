import {afterEach,expect,it,vi} from 'vitest';
import {readAndClearPush} from './readPush';
afterEach(()=>vi.unstubAllGlobals());
it('closes only matching displayed notices after an acknowledged read',async()=>{
 const close=vi.fn(),other=vi.fn(),later=vi.fn();let list=[{data:{target:'/mobile/notifications?event=7'},close},{data:{target:'/mobile/notifications?event=8'},close:other}];
 vi.stubGlobal('navigator',{serviceWorker:{getRegistration:async()=>({getNotifications:async()=>[...list]})}});
 await readAndClearPush(async()=>{expect(close).not.toHaveBeenCalled();list.push({data:{target:'/mobile/notifications?event=7'},close:later});return true;},(_d,u)=>u.searchParams.get('event')==='7');
 await vi.waitFor(()=>expect(close).toHaveBeenCalledOnce());expect(other).not.toHaveBeenCalled();expect(later).not.toHaveBeenCalled();
});
it('does not close notifications when committing the read fails',async()=>{
 const close=vi.fn();vi.stubGlobal('navigator',{serviceWorker:{getRegistration:async()=>({getNotifications:async()=>[{data:{target:'/mobile'},close}]})}});
 await expect(readAndClearPush(async()=>{throw Error('offline');},()=>true)).rejects.toThrow('offline');expect(close).not.toHaveBeenCalled();
});
it('keeps read working when system notification APIs are unavailable',async()=>{vi.stubGlobal('navigator',{});expect(await readAndClearPush(async()=>42,()=>true)).toBe(42);});
