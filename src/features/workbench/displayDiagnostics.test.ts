import {afterEach,expect,it,vi} from "vitest";
import {displayDiagnostics,recordSheetEvidence} from "./displayDiagnostics";

afterEach(()=>{document.body.innerHTML="";vi.unstubAllGlobals();});
it("retains closed sheet geometry without reading private messages, input or target IDs",()=>{
 document.body.innerHTML='<div class="v4-chat-sheet-frame"><div class="v4-chat-sheet" data-thread-id="PRIVATE_THREAD"><p>PRIVATE_MESSAGE</p><div class="v4-chat-sheet-body"><button>PRIVATE_ATTACHMENT</button><textarea>PRIVATE_DRAFT</textarea></div></div></div>';
 const button=document.querySelector('button')!;
 vi.spyOn(button,'getBoundingClientRect').mockReturnValue({top:700,left:20,width:350,height:48} as DOMRect);
 const fetch=vi.fn();vi.stubGlobal('fetch',fetch);
 recordSheetEvidence('添加附件');document.body.innerHTML='';
 const text=displayDiagnostics(),report=JSON.parse(text);
 expect(report.lastSheets['添加附件'].buttons[0].box).toEqual({top:700,left:20,width:350,height:48});
 for(const privateValue of ['PRIVATE_THREAD','PRIVATE_MESSAGE','PRIVATE_ATTACHMENT','PRIVATE_DRAFT'])expect(text).not.toContain(privateValue);
 expect(fetch).not.toHaveBeenCalled();
});
