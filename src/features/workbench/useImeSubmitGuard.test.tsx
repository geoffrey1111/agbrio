import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {useImeSubmitGuard} from './useImeSubmitGuard';
afterEach(()=>{cleanup();vi.restoreAllMocks();});
it('keeps composition, WebKit 229 and the trailing commit event out of keyboard submission',()=>{
 let now=1000;vi.spyOn(performance,'now').mockImplementation(()=>now);const send=vi.fn();
 function Composer(){const ime=useImeSubmitGuard();return <textarea aria-label="reply" onCompositionStart={ime.onCompositionStart} onCompositionEnd={ime.onCompositionEnd} onKeyDown={e=>{if(e.ctrlKey&&e.key==='Enter'&&!ime.blocks(e.nativeEvent))send();}}/>;}
 render(<Composer/>);const input=screen.getByRole('textbox');fireEvent.compositionStart(input);fireEvent.keyDown(input,{key:'Enter',ctrlKey:true,isComposing:false});expect(send).not.toHaveBeenCalled();
 fireEvent.compositionEnd(input);fireEvent.keyDown(input,{key:'Enter',ctrlKey:true,keyCode:229});now+=20;fireEvent.keyDown(input,{key:'Enter',ctrlKey:true});expect(send).not.toHaveBeenCalled();
 now+=100;fireEvent.keyDown(input,{key:'Enter',ctrlKey:true});expect(send).toHaveBeenCalledOnce();
});
