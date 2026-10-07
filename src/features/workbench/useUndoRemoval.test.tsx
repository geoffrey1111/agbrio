import {renderHook,act,cleanup} from "@testing-library/react";
import {it,expect,vi,afterEach} from "vitest";
import {useUndoRemoval} from "./useUndoRemoval";
afterEach(cleanup);
it("returns authority to server after Undo so a later remote removal stays removed",async()=>{
 const row={id:"exact"};const persist=vi.fn().mockResolvedValue(undefined);const {result}=renderHook(()=>useUndoRemoval<typeof row>(r=>r.id,persist,vi.fn()));
 await act(async()=>{result.current.remove(row);});expect(result.current.project([])).toEqual([]);
 await act(async()=>{result.current.restore(row);});expect(persist).toHaveBeenLastCalledWith(row,false);
 expect(result.current.project([row])).toEqual([row]);expect(result.current.project([])).toEqual([]);
});
