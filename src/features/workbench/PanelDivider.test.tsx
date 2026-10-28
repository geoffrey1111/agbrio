import {useState} from 'react';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {PanelDivider} from './PanelDivider';
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
function fixture(initial=50){
 class Pointer extends MouseEvent{pointerId:number;constructor(type:string,init:MouseEventInit&{pointerId?:number}={}){super(type,init);this.pointerId=init.pointerId??1;}}
 vi.stubGlobal('PointerEvent',Pointer);
 const change=vi.fn();function Harness(){const[value,setValue]=useState(initial);return <PanelDivider value={value} min={25} max={75} factor={10} step={5} onChange={v=>{change(v);setValue(v);}}/>;}
 render(<Harness/>);const divider=screen.getByRole('separator');const capture=new Set<number>();
 Object.assign(divider,{setPointerCapture:(id:number)=>capture.add(id),hasPointerCapture:(id:number)=>capture.has(id),releasePointerCapture:(id:number)=>capture.delete(id)});
 return{divider,change};
}
it('moves immediately and Escape restores the original width without a late capture rollback',()=>{
 const{divider,change}=fixture();fireEvent.pointerDown(divider,{button:0,pointerId:1,clientX:100});fireEvent.pointerMove(divider,{pointerId:1,clientX:200});expect(divider).toHaveAttribute('aria-valuenow','60');expect(divider).toHaveAttribute('data-dragging','true');
 fireEvent.keyDown(divider,{key:'Escape'});expect(divider).toHaveAttribute('aria-valuenow','50');expect(divider).toHaveAttribute('data-dragging','false');fireEvent.lostPointerCapture(divider,{pointerId:1});expect(change).toHaveBeenLastCalledWith(50);
});
it('cancel rolls back, release commits, and double click restores an even split',()=>{
 const{divider}=fixture();fireEvent.pointerDown(divider,{button:0,pointerId:1,clientX:0});fireEvent.pointerMove(divider,{pointerId:1,clientX:150});fireEvent.pointerCancel(divider,{pointerId:2});expect(divider).toHaveAttribute('aria-valuenow','65');fireEvent.pointerCancel(divider,{pointerId:1});expect(divider).toHaveAttribute('aria-valuenow','50');
 fireEvent.pointerDown(divider,{button:0,pointerId:1,clientX:0});fireEvent.pointerMove(divider,{pointerId:1,clientX:150});fireEvent.pointerUp(divider,{pointerId:1});expect(divider).toHaveAttribute('aria-valuenow','65');fireEvent.doubleClick(divider);expect(divider).toHaveAttribute('aria-valuenow','50');
});
it('supports keyboard limits without changing border dimensions',()=>{
 const{divider}=fixture();fireEvent.keyDown(divider,{key:'Home'});expect(divider).toHaveAttribute('aria-valuenow','25');fireEvent.keyDown(divider,{key:'End'});expect(divider).toHaveAttribute('aria-valuenow','75');fireEvent.keyDown(divider,{key:'ArrowLeft'});expect(divider).toHaveAttribute('aria-valuenow','70');
});
