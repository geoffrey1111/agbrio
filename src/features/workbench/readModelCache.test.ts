import {afterEach,expect,it,vi} from "vitest";
import {ReadModelCache} from "./readModelCache";
const caches:ReadModelCache[]=[];
function cache(){const c=new ReadModelCache(()=>localStorage);caches.push(c);return c;}
afterEach(()=>{for(const c of caches.splice(0))c.dispose();localStorage.clear();vi.useRealTimers();vi.restoreAllMocks();});
it("deduplicates cold reads and returns a warm snapshot without another request",async()=>{
 const c=cache();let done!:(v:string)=>void;const load=vi.fn(()=>new Promise<string>(r=>done=r));
 const a=c.read("bridge:a",load),b=c.read("bridge:a",load);await Promise.resolve();expect(load).toHaveBeenCalledTimes(1);done("full body");expect(await a).toBe("full body");expect(await b).toBe("full body");expect(await c.read("bridge:a",load)).toBe("full body");expect(load).toHaveBeenCalledTimes(1);
});
it("updates a mounted reader quietly on its schedule and survives a failed refresh",async()=>{
 vi.useFakeTimers();const c=cache(),changed=vi.fn();c.subscribe("chat:a",changed);const load=vi.fn().mockResolvedValueOnce("old").mockResolvedValueOnce("new").mockRejectedValue(new Error("offline"));
 await c.read("chat:a",load,5000);await vi.advanceTimersByTimeAsync(5000);expect(c.peek("chat:a")).toBe("new");expect(changed).toHaveBeenCalledTimes(2);await vi.advanceTimersByTimeAsync(5000);expect(c.peek("chat:a")).toBe("new");expect(await c.read("chat:a",load)).toBe("new");expect(load).toHaveBeenCalledTimes(3);
});
it("warms unopened resources two at a time and pauses while hidden",async()=>{
 vi.useFakeTimers();const c=cache();const load=vi.fn().mockResolvedValue("ready");Object.defineProperty(document,"visibilityState",{configurable:true,value:"hidden"});for(let n=0;n<5;n++)c.warm(`bridge:${n}`,load);
 await vi.advanceTimersByTimeAsync(2000);expect(load).not.toHaveBeenCalled();Object.defineProperty(document,"visibilityState",{configurable:true,value:"visible"});document.dispatchEvent(new Event("visibilitychange"));await vi.advanceTimersByTimeAsync(0);expect(load).toHaveBeenCalledTimes(2);await vi.advanceTimersByTimeAsync(1000);expect(load).toHaveBeenCalledTimes(4);
});
it("restores bounded private snapshots only for the same live session scope",async()=>{
 vi.useFakeTimers();const first=cache();first.setScope("device:a");first.put("bridge:a",{text:"private body"});await vi.advanceTimersByTimeAsync(250);first.dispose();const reopened=cache();reopened.setScope("device:a");expect(reopened.peek("bridge:a")).toEqual({text:"private body"});reopened.setScope("device:b");expect(reopened.peek("bridge:a")).toBeUndefined();expect(localStorage.getItem("agbrio.private-read-cache.v1")).toBeNull();
});
it("logout fences an outstanding old-session read and removes persistent content",async()=>{
 const c=cache();c.setScope("device:a");let done!:(v:string)=>void;const request=c.read("chat:a",()=>new Promise<string>(r=>done=r));await Promise.resolve();c.clear();c.setScope("device:b");done("private old result");await expect(request).rejects.toThrow("READ_CACHE_SUPERSEDED");expect(c.peek("chat:a")).toBeUndefined();
});
it("binding mutation invalidates an old pending read instead of restoring old data",async()=>{
 const c=cache();let done!:(v:string)=>void;const request=c.read("bridge:a",()=>new Promise<string>(r=>done=r));await Promise.resolve();c.invalidate("bridge:a",true);done("old endpoint");await expect(request).rejects.toThrow("READ_CACHE_SUPERSEDED");expect(c.peek("bridge:a")).toBeUndefined();await c.refresh("bridge:a",async()=>"new endpoint");expect(c.peek("bridge:a")).toBe("new endpoint");
});
it("expired, corrupt and quota-limited storage do not block fresh reads",async()=>{
 vi.useFakeTimers();localStorage.setItem("agbrio.private-read-cache.v1",JSON.stringify({scope:"a",rows:[{key:"bridge:a",at:Date.now()-86400001,value:"expired"}]}));const c=cache();c.setScope("a");expect(c.peek("bridge:a")).toBeUndefined();vi.spyOn(Storage.prototype,"setItem").mockImplementation(()=>{throw Error("quota");});expect(await c.read("bridge:a",async()=>"fresh")).toBe("fresh");await vi.advanceTimersByTimeAsync(250);expect(c.peek("bridge:a")).toBe("fresh");
});
it("does not persist unscoped or oversized bodies",async()=>{
 vi.useFakeTimers();const c=cache();c.put("chat:a","unscoped");await vi.advanceTimersByTimeAsync(250);expect(localStorage.length).toBe(0);c.setScope("a");c.put("chat:b","x".repeat(1600000));c.put("bridge:a","small");await vi.advanceTimersByTimeAsync(250);const stored=localStorage.getItem("agbrio.private-read-cache.v1")!;expect(stored.length).toBeLessThan(1500000);expect(stored).toContain("small");expect(stored).not.toContain("chat:b");
});
it("a visible online wake retries a failed refresh immediately, without waiting for backoff",async()=>{
 vi.useFakeTimers();const c=cache();c.subscribe("bridge:a",()=>{});const load=vi.fn().mockResolvedValueOnce("old").mockRejectedValueOnce(Error("offline")).mockResolvedValue("current");await c.read("bridge:a",load,2000);await vi.advanceTimersByTimeAsync(2000);expect(c.peek("bridge:a")).toBe("old");window.dispatchEvent(new Event("online"));await vi.advanceTimersByTimeAsync(0);expect(c.peek("bridge:a")).toBe("current");expect(load).toHaveBeenCalledTimes(3);
});
it("entering a warm resource upgrades its slower background cadence",async()=>{
 vi.useFakeTimers();const c=cache(),load=vi.fn().mockResolvedValue("body");c.warm("bridge:a",load,30000);await vi.advanceTimersByTimeAsync(1000);expect(load).toHaveBeenCalledTimes(1);c.subscribe("bridge:a",()=>{});await c.read("bridge:a",load,2000);await vi.advanceTimersByTimeAsync(2000);expect(load).toHaveBeenCalledTimes(2);
});
it("navigation reads never refresh an expired warm snapshot, even after invalidation",async()=>{
 vi.useFakeTimers();const c=cache(),load=vi.fn().mockResolvedValue("body");await c.read("bridge:a",load,5000);const at=c.observedAt("bridge:a");
 vi.setSystemTime(Date.now()+16000);for(let n=0;n<8;n++)expect(await c.read("bridge:a",load,5000)).toBe("body");c.invalidate("bridge:a");expect(await c.read("bridge:a",load,5000)).toBe("body");expect(load).toHaveBeenCalledTimes(1);expect(c.observedAt("bridge:a")).toBe(at);
 await vi.advanceTimersByTimeAsync(1000);expect(load).toHaveBeenCalledTimes(2);
});
it("a resource keeps the same synchronization period after its last page listener leaves",async()=>{
 vi.useFakeTimers();const c=cache(),load=vi.fn().mockResolvedValue("body"),stop=c.subscribe("bridge:a",()=>{});await c.read("bridge:a",load,5000);stop();await vi.advanceTimersByTimeAsync(10000);expect(load).toHaveBeenCalledTimes(3);
});

it("desktop keeps its read clock hidden and does not queue every entry again on focus",async()=>{
 vi.useFakeTimers();const c=new ReadModelCache(()=>undefined,true);caches.push(c);const load=vi.fn().mockResolvedValue("body");await c.read("bridge:a",load,5000);Object.defineProperty(document,"visibilityState",{configurable:true,value:"hidden"});await vi.advanceTimersByTimeAsync(15000);expect(load).toHaveBeenCalledTimes(4);Object.defineProperty(document,"visibilityState",{configurable:true,value:"visible"});window.dispatchEvent(new Event("focus"));await vi.advanceTimersByTimeAsync(0);expect(load).toHaveBeenCalledTimes(4);expect(c.age("bridge:a")).toBe(0);
});
