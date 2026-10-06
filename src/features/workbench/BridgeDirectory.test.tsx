// Historical V3/V5 projection contract. Native R2 is tested in NativeWorkbench.test.tsx.
import {cleanup,fireEvent,render,screen} from "@testing-library/react";import{afterEach,expect,it,vi}from"vitest";import{LegacyWorkbench as UnifiedWorkbench}from"./UnifiedWorkbench";
afterEach(cleanup);
it("returns to all Bridges and routes by exact ID without transmitting",()=>{
 const select=vi.fn(),navigate=vi.fn();render(<UnifiedWorkbench items={[{id:"one",name:"同名",lifecycle:"ACTIVE"},{id:"two",name:"同名",lifecycle:"ACTIVE"}]} selectedWorkstreamId="one" surface="BRIDGES" draft={{value:""}} onDraftChange={vi.fn()} onSelectWorkstream={select} onSurfaceChange={navigate}/>);const list=screen.getByRole("region",{name:"Bridge 列表"});const buttons=list.querySelectorAll('.v5-bridge-row');fireEvent.click(buttons[1]);expect(select).toHaveBeenCalledWith("two");fireEvent.click(screen.getByRole("button",{name:"Bridge"}));expect(navigate).toHaveBeenCalledWith("BRIDGES");
});
it("allows keyboard resizing with bounded panel width",()=>{
 render(<UnifiedWorkbench items={[]} surface="BRIDGES" draft={{value:""}} onDraftChange={vi.fn()} onSelectWorkstream={vi.fn()}/>);const divider=screen.getByRole("separator",{name:"调整列表宽度"});fireEvent.keyDown(divider,{key:"ArrowRight"});expect(divider).toHaveAttribute("aria-valuenow","256");
});
