import {useRef} from "react";
import {cleanup,render,screen} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {useSurfaceTransition} from "./InteractionMotion";

function Fixture({view,detail=false,refresh=0,keyboard=false}:{view:string;detail?:boolean;refresh?:number;keyboard?:boolean}){
 const ref=useRef<HTMLElement>(null);
 useSurfaceTransition(ref,view,detail,keyboard);
 return <section ref={ref}><input aria-label="draft" defaultValue="preserved"/><span>{refresh}</span></section>;
}
const originalAnimate=Object.getOwnPropertyDescriptor(HTMLElement.prototype,"animate");
afterEach(()=>{cleanup();vi.restoreAllMocks();if(originalAnimate)Object.defineProperty(HTMLElement.prototype,"animate",originalAnimate);else Reflect.deleteProperty(HTMLElement.prototype,"animate");});
function platform(reduced=false){
 const cancel=vi.fn(),animate=vi.fn(()=>({cancel})),remove=vi.fn();
 let change=()=>{};
 vi.spyOn(window,"matchMedia").mockImplementation(()=>({matches:reduced,addEventListener:(_event:string,cb:()=>void)=>{change=cb;},removeEventListener:remove}) as unknown as MediaQueryList);
 Object.defineProperty(HTMLElement.prototype,"animate",{configurable:true,value:animate});
 return {animate,cancel,remove,change:()=>change()};
}
it("does not animate first load or passive data refresh; navigation preserves the draft node",()=>{
 const p=platform();const {rerender}=render(<Fixture view="directory"/>);const input=screen.getByRole("textbox");
 rerender(<Fixture view="directory" refresh={1}/>);expect(p.animate).not.toHaveBeenCalled();
 rerender(<Fixture view="reader" detail/>);expect(p.animate).toHaveBeenCalledTimes(1);expect(screen.getByRole("textbox")).toBe(input);expect(input).toHaveValue("preserved");
 rerender(<Fixture view="reader" detail refresh={2}/>);expect(p.animate).toHaveBeenCalledTimes(1);
});
it("cancels previous navigation and tears down listeners on rapid navigation/unmount",()=>{
 const p=platform();const {rerender,unmount}=render(<Fixture view="list"/>);
 rerender(<Fixture view="reader" detail/>);rerender(<Fixture view="settings"/>);expect(p.cancel).toHaveBeenCalledTimes(1);
 p.change();expect(p.cancel).toHaveBeenCalledTimes(2);unmount();expect(p.remove).toHaveBeenCalledTimes(2);
});
it("honors reduced motion and keyboard navigation without delaying navigation",()=>{
 const p=platform(true);const {rerender}=render(<Fixture view="list"/>);
 rerender(<Fixture view="reader" detail/>);expect(p.animate).not.toHaveBeenCalled();
 rerender(<Fixture view="settings" keyboard/>);expect(p.animate).not.toHaveBeenCalled();
});
