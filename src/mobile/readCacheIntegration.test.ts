import {afterEach,expect,it,vi} from "vitest";
import {readModels} from "../features/workbench/readModelCache";
import {mobileRoleBridgeApi} from "./roleBridgeApi";
afterEach(()=>{readModels.clear();readModels.dispose();vi.unstubAllGlobals();vi.useRealTimers();});
it("passive POST synchronization publishes newer Bridge content rather than invalidating its own pending read",async()=>{
 vi.useFakeTimers();readModels.clear();const api=mobileRoleBridgeApi("exact-work");let version=1;
 const fetch=vi.fn(async(_input:RequestInfo|URL,_init?:RequestInit)=>new Response(JSON.stringify({bindings:{workstreamId:"exact-work",bindingRevision:1,explicitRoles:true},replies:[{id:"observation",endpointId:"endpoint",text:`body${version}`}],handoffs:[],snapshotAt:Date.now()})));
 vi.stubGlobal("fetch",fetch);const seen=vi.fn();const stop=api.subscribeState!("exact-work",seen);
 expect((await api.state("exact-work")).replies[0].text).toBe("body1");version=2;await vi.advanceTimersByTimeAsync(2500);await api.sync!("exact-work");await vi.advanceTimersByTimeAsync(0);
 expect(api.cachedState!("exact-work")?.replies[0].text).toBe("body2");expect(seen).toHaveBeenCalledTimes(2);expect(fetch.mock.calls.at(-1)?.[1]).toMatchObject({method:"POST",body:JSON.stringify({action:"SYNC"})});stop();
});
