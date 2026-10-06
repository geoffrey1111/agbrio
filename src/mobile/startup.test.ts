import {afterEach,expect,it,vi} from "vitest";

afterEach(()=>{vi.unstubAllGlobals();vi.resetModules();});

it("shares the one live bootstrap request across the initial effect replay, then rechecks",async()=>{
 let release!:(value:Response)=>void;
 const fetch=vi.fn().mockImplementationOnce(()=>new Promise<Response>(resolve=>release=resolve)).mockResolvedValue(new Response(JSON.stringify({authenticated:false})));
 vi.stubGlobal("fetch",fetch);
 const{prepareMobileSession,readMobileSession}=await import("./startup");
 prepareMobileSession();expect(fetch).toHaveBeenCalledTimes(1);
 const first=readMobileSession(),replay=readMobileSession();expect(first).toBe(replay);
 release(new Response(JSON.stringify({authenticated:true,method:"DEVICE"})));
 expect(await replay).toEqual({authenticated:true,method:"DEVICE"});
 expect(await readMobileSession()).toEqual({authenticated:false});expect(fetch).toHaveBeenCalledTimes(2);
 expect(fetch.mock.calls[0][1]).toMatchObject({credentials:"same-origin",cache:"no-store"});
});

it("does not turn an initial network failure into a saved login and can retry live",async()=>{
 const fetch=vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(new Response(JSON.stringify({authenticated:false})));
 vi.stubGlobal("fetch",fetch);
 const{prepareMobileSession,readMobileSession}=await import("./startup");
 prepareMobileSession();await expect(readMobileSession()).rejects.toThrow("offline");
 expect(await readMobileSession()).toEqual({authenticated:false});
});
