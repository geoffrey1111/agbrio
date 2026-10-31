import {renderHook,act,cleanup} from "@testing-library/react";
import {it,expect,vi,afterEach} from "vitest";
import {useDirectorySync} from "./useDirectorySync";
afterEach(()=>{cleanup();vi.useRealTimers();});
it("refreshes shared list state on foreground and each interval without overlapping calls",async()=>{
 vi.useFakeTimers();let finish!:()=>void;const first=new Promise<void>(resolve=>{finish=resolve;});const refresh=vi.fn().mockReturnValueOnce(first).mockResolvedValue(undefined);renderHook(()=>useDirectorySync(refresh));
 act(()=>window.dispatchEvent(new Event("focus")));expect(refresh).toHaveBeenCalledTimes(1);await act(async()=>{await vi.advanceTimersByTimeAsync(10000);});expect(refresh).toHaveBeenCalledTimes(1);
 await act(async()=>{finish();});await act(async()=>{await vi.advanceTimersByTimeAsync(5000);});expect(refresh).toHaveBeenCalledTimes(2);
});

it("checks on mount and pageshow instead of waiting for the five-second clock",async()=>{
 vi.useFakeTimers();const refresh=vi.fn().mockResolvedValue(undefined);renderHook(()=>useDirectorySync(refresh,{onMount:true}));expect(refresh).toHaveBeenCalledOnce();await act(async()=>{});
 await act(async()=>{window.dispatchEvent(new Event("pageshow"));});expect(refresh).toHaveBeenCalledTimes(2);await act(async()=>{window.dispatchEvent(new Event("online"));});expect(refresh).toHaveBeenCalledTimes(3);
});
