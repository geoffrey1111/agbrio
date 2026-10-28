import {afterEach,expect,it,vi} from "vitest";
import {cachedNotificationApi} from "./notificationApi";
import type {NotificationApi,CodexWatch} from "./CodexNotifications";
import {readModels} from "./readModelCache";
afterEach(()=>{readModels.clear();readModels.dispose();});
it('keeps an acknowledged list restoration when the follow-up read is offline',async()=>{
 const{raw,watch}=fixture();raw.show=vi.fn().mockResolvedValue({shownAt:12345});const api=cachedNotificationApi(raw);await api.watches();vi.mocked(raw.watches).mockRejectedValue(new Error('offline'));
 await expect(api.show!(watch.threadId)).resolves.toEqual({shownAt:12345});expect(api.cachedWatches!()![0]).toMatchObject({shownAt:12345,generation:1});expect(raw.enable).not.toHaveBeenCalled();
});
function fixture(){const watch:CodexWatch={threadId:"fictional",label:"A",cwd:"D:/fictional",enabled:true,generation:1,checkedAt:1,errorCode:null,snapshot:{state:"IDLE",turnId:null,itemId:null,text:""}};const raw:NotificationApi={watches:vi.fn().mockResolvedValue([watch]),pause:vi.fn().mockResolvedValue({}),enable:vi.fn().mockResolvedValue({}),connect:vi.fn(),threads:vi.fn(),feed:vi.fn(),event:vi.fn(),webUrl:vi.fn()};return{raw,watch};}
it("returns acknowledged pause state immediately instead of stale cached enabled state",async()=>{
 const{raw,watch}=fixture();const api=cachedNotificationApi(raw);await api.watches();vi.mocked(raw.watches).mockResolvedValue([{...watch,enabled:false,generation:2}]);await api.pause(watch.threadId);expect((await api.watches())[0]).toMatchObject({enabled:false,generation:2});expect(raw.pause).toHaveBeenCalledOnce();
});
it("does not misreport a successful pause if the subsequent list read is offline",async()=>{
 const{raw,watch}=fixture();const api=cachedNotificationApi(raw);await api.watches();vi.mocked(raw.watches).mockRejectedValue(new Error("offline"));await expect(api.pause(watch.threadId)).resolves.toEqual({});expect(api.cachedWatches!()![0].enabled).toBe(false);expect(raw.pause).toHaveBeenCalledOnce();
});
