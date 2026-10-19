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
it("tags an unauthorized response with its initiating session generation",async()=>{
 let finish!:(r:Response)=>void;vi.stubGlobal("fetch",()=>new Promise<Response>(r=>finish=r));const generation=readModels.generation();let observed:number|undefined;const capture=(e:Event)=>{observed=(e as CustomEvent).detail.generation;};window.addEventListener("aiwr-login-required",capture);
 try{const pending=liveRequest("/codex-watches");readModels.clear();finish(new Response('{"error":"unauthorized"}',{status:401}));await expect(pending).rejects.toBeInstanceOf(MobileApiError);expect(observed).toBe(generation);expect(observed).not.toBe(readModels.generation());}finally{window.removeEventListener("aiwr-login-required",capture);}
});
