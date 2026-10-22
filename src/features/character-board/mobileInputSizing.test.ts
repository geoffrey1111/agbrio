import {afterEach,expect,it,vi} from 'vitest';
import {sizeBoardInputs} from './mobileInputSizing';
import {boardDocument} from './boardDocument';
// Vitest stubs CSS imports; exercise the actual bundled text used by iframe pages.
vi.mock('../../styles/mobile-inputs.css?raw',async()=>({default:(await import('node:fs')).readFileSync((await import('node:path')).resolve(process.cwd(),'src/styles/mobile-inputs.css'),'utf8')}));
afterEach(()=>document.body.replaceChildren());
it('adds only a presentation rule to an accessible board, once',()=>{
 const frame=document.createElement('iframe');document.body.append(frame);
 sizeBoardInputs(frame);sizeBoardInputs(frame);
 expect(frame.contentDocument?.querySelectorAll('#agbrio-mobile-inputs')).toHaveLength(1);
 expect(frame.contentDocument?.querySelector('style')?.textContent).toContain('font-size:max(16px,1rem)!important');
});
it('keeps foreign-origin boards untouched when the browser refuses access',()=>{
 const frame=document.createElement('iframe');vi.spyOn(frame,'contentDocument','get').mockImplementation(()=>{throw new DOMException('Blocked','SecurityError');});
 expect(()=>sizeBoardInputs(frame)).not.toThrow();
});
it('covers the opaque local-board projection without changing its source document',()=>{
 const html='<html><head></head><body><textarea>draft</textarea></body></html>';
 const result=boardDocument(html);expect(result).toContain('id="agbrio-mobile-inputs"');expect(result).toContain('<textarea>draft</textarea>');
 expect(result).not.toContain('user-scalable=no');expect(result).not.toContain('maximum-scale=1');
});
