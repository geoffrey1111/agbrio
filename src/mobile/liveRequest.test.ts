import {afterEach,expect,it,vi} from "vitest";
import {liveRequest} from "./api";
import {MobileApiError} from "./api";
import {readModels} from "../features/workbench/readModelCache";
afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();readModels.clear();readModels.dispose();});
it("bounds a stalled read instead of leaving the page loading forever",async()=>{
 vi.useFakeTimers();const fetcher=vi.fn((_url:string,init:RequestInit)=>new Promise<Response>((_resolve,reject)=>init.signal?.addEventListener("abort",()=>reject(new DOMException("Aborted","AbortError")))));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest("/workstreams");const rejected=expect(pending).rejects.toMatchObject({status:408});await vi.advanceTimersByTimeAsync(15001);await rejected;expect(fetcher).toHaveBeenCalledOnce();
});
it("does not timeout, retry or duplicate a write",async()=>{
 vi.useFakeTimers();let finish!:(r:Response)=>void;const fetcher=vi.fn(()=>new Promise<Response>(r=>finish=r));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest("/example-command",{method:"POST",body:"{}"});await vi.advanceTimersByTimeAsync(60000);expect(fetcher).toHaveBeenCalledOnce();finish(new Response("{}",{headers:{"content-type":"application/json"}}));await pending;
});
it.each([
 ["/bridge-activity",{workstreamIds:["fixture"]}],
 ["/claude-code/chat",{action:"HISTORY"}],
 ["/claude-code/chat",{action:"CHILD_HISTORY"}],
 ["/codex-watches/chat",{action:"HISTORY"}],
 ["/workstreams/fixture/role-bridge",{action:"SYNC"}],
 ["/workstreams/fixture/role-bridge",{action:"READ",role:"DECISION"}],
 ["/workstreams/fixture/role-bridge",{action:"THREADS"}],
 ["/workstreams/fixture/role-bridge",{action:"BLOCKS"}],
 ["/workstreams/fixture/role-bridge",{action:"ATTACHMENTS"}],
])("bounds the passive POST read %s instead of wedging refresh",async(path,body)=>{
 vi.useFakeTimers();const fetcher=vi.fn((_url:string,init:RequestInit)=>new Promise<Response>((_resolve,reject)=>init.signal?.addEventListener("abort",()=>reject(new DOMException("Aborted","AbortError")))));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest(path,{method:"POST",body:JSON.stringify(body)});const rejected=expect(pending).rejects.toMatchObject({status:408});
 await vi.advanceTimersByTimeAsync(15001);await rejected;expect(fetcher).toHaveBeenCalledOnce();
});
it("allows the next passive read to recover after a timeout without replaying the failed read",async()=>{
 vi.useFakeTimers();const fetcher=vi.fn().mockImplementationOnce((_url:string,init:RequestInit)=>new Promise<Response>((_resolve,reject)=>init.signal?.addEventListener("abort",()=>reject(new DOMException("Aborted","AbortError"))))).mockResolvedValue(new Response('{"ready":true}'));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest("/bridge-activity",{method:"POST",body:'{"workstreamIds":[]}'});const rejected=expect(pending).rejects.toMatchObject({status:408});await vi.advanceTimersByTimeAsync(15001);await rejected;
 await expect(liveRequest("/bridge-activity",{method:"POST",body:'{"workstreamIds":[]}'})).resolves.toEqual({ready:true});expect(fetcher).toHaveBeenCalledTimes(2);
});
it("cancels a suspended passive read without waiting for its deadline",async()=>{
 const fetcher=vi.fn((_url:string,init:RequestInit)=>new Promise<Response>((_resolve,reject)=>init.signal?.addEventListener("abort",()=>reject(init.signal!.reason))));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest("/bridge-activity",{method:"POST",body:'{"workstreamIds":[]}'});const rejected=expect(pending).rejects.toMatchObject({name:"AbortError",message:"ROUTER_READ_SUSPENDED"});window.dispatchEvent(new Event("pagehide"));await rejected;expect(fetcher).toHaveBeenCalledOnce();
});
it("does not cancel or replay a command when the page is suspended",async()=>{
 let finish!:(r:Response)=>void;const fetcher=vi.fn((_url:string,_init?:RequestInit)=>new Promise<Response>(r=>finish=r));vi.stubGlobal("fetch",fetcher);
 const pending=liveRequest("/example-command",{method:"POST",body:"{}"});window.dispatchEvent(new Event("pagehide"));expect(fetcher).toHaveBeenCalledOnce();expect(fetcher.mock.calls[0][1]?.signal).toBeUndefined();finish(new Response("{}"));await pending;
});
it("tags an unauthorized response with its initiating session generation",async()=>{
 let finish!:(r:Response)=>void;vi.stubGlobal("fetch",()=>new Promise<Response>(r=>finish=r));const generation=readModels.generation();let observed:number|undefined;const capture=(e:Event)=>{observed=(e as CustomEvent).detail.generation;};window.addEventListener("aiwr-login-required",capture);
 try{const pending=liveRequest("/codex-watches");readModels.clear();finish(new Response('{"error":"unauthorized"}',{status:401}));await expect(pending).rejects.toBeInstanceOf(MobileApiError);expect(observed).toBe(generation);expect(observed).not.toBe(readModels.generation());}finally{window.removeEventListener("aiwr-login-required",capture);}
});
