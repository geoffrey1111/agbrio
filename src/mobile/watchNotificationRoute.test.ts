import {it,expect} from 'vitest';
import {watchNotificationRoute} from './watchNotificationRoute';
it('admits only same-origin exact positive safe event ids',()=>{
 expect(watchNotificationRoute('/mobile/notifications?event=42','https://router.example')?.sequence).toBe(42);
 for(const input of ['https://evil.example/mobile/notifications?event=42','/mobile?event=42','/mobile/notifications?event=0','/mobile/notifications?event=-1','/mobile/notifications?event=1.2','/mobile/notifications?event=9007199254740992'])expect(watchNotificationRoute(input,'https://router.example')).toBeNull();
});
