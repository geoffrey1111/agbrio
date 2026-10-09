import {act,cleanup,fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {useState} from "react";
import {UnifiedWorkbench,type UnifiedWorkbenchProps,type WorkbenchSurface} from "./UnifiedWorkbench";

afterEach(()=>{cleanup();localStorage.clear();});
const base:UnifiedWorkbenchProps={items:[{id:"exact-a",name:"同名 Bridge",lifecycle:"ACTIVE"},{id:"exact-b",name:"同名 Bridge",lifecycle:"ACTIVE"}],draft:{value:""},onDraftChange:()=>{},onSelectWorkstream:()=>{}};
it('offers interface updates in mobile About without replacing desktop installer controls',()=>{
 const previous=location.pathname;history.replaceState({},'', '/mobile');
 try{render(<UnifiedWorkbench {...base} surface='SETTINGS'/>);fireEvent.click(screen.getByRole('button',{name:'关于'}));expect(screen.getByRole('button',{name:'检查界面更新'})).toBeVisible();}
 finally{history.replaceState({},'',previous);}
});
it("has three stable destinations and routes same-named Bridges by their own ID",()=>{
 const select=vi.fn();render(<UnifiedWorkbench {...base} onSelectWorkstream={select}/>);
 const nav=screen.getByRole("navigation",{name:"桌面导航"});
 expect(within(nav).getAllByRole("button").map(button=>button.textContent)).toEqual(["Bridge","通知","设置"]);
 expect(screen.queryByRole("button",{name:"项目"})).not.toBeInTheDocument();
 const list=screen.getByText("最近").parentElement!;fireEvent.click(within(list).getAllByRole("button",{name:/^同名 Bridge/})[1]);expect(select).toHaveBeenCalledWith("exact-b");
});
it("keeps the role reader mounted but hidden while navigating utilities",()=>{
 const {rerender}=render(<UnifiedWorkbench {...base} selectedWorkstreamId="exact-a" surface="WORKSPACE" bridgePanel={<div>exact role content</div>}/>);
 expect(screen.getByText("exact role content")).toBeVisible();
 rerender(<UnifiedWorkbench {...base} selectedWorkstreamId="exact-a" surface="SETTINGS" bridgePanel={<div>exact role content</div>}/>);
 expect(screen.getByText("exact role content")).not.toBeVisible();expect(screen.getByRole("button",{name:"设备"})).toBeVisible();
});
it("removes immediately without confirmation, then rolls back a failed persistence",async()=>{
 let reject!:(cause:Error)=>void;const remove=vi.fn(()=>new Promise<void>((_,fail)=>{reject=fail;}));
 render(<UnifiedWorkbench {...base} items={[base.items[0]]} onLifecycleChange={remove}/>);
 fireEvent.click(screen.getByRole("button",{name:"删除 同名 Bridge"}));
 expect(screen.queryByRole("dialog")).toBeNull();expect(screen.queryByRole("button",{name:/^同名 Bridge/})).toBeNull();
 expect(screen.getByText("已移除 Bridge")).toBeInTheDocument();
 await waitFor(()=>expect(remove).toHaveBeenCalledWith("exact-a","TRASHED"));
 reject(new Error("busy"));await screen.findByText("删除未完成，条目已恢复。");expect(screen.getAllByRole("button",{name:/^同名 Bridge/}).length).toBeGreaterThan(0);
});
it("serializes an immediate undo after a pending deletion and preserves the exact identity",async()=>{
 let finish!:()=>void;const change=vi.fn().mockImplementationOnce(()=>new Promise<void>(resolve=>{finish=resolve;})).mockResolvedValue(undefined);
 render(<UnifiedWorkbench {...base} items={[base.items[0]]} onLifecycleChange={change}/>);
 fireEvent.click(screen.getByRole("button",{name:"删除 同名 Bridge"}));
 await waitFor(()=>expect(change).toHaveBeenCalledTimes(1));
 fireEvent.click(screen.getByRole("button",{name:"撤销"}));expect(screen.getAllByRole("button",{name:/^同名 Bridge/}).length).toBeGreaterThan(0);expect(change).toHaveBeenCalledTimes(1);
 finish();await waitFor(()=>expect(change).toHaveBeenLastCalledWith("exact-a","ACTIVE"));expect(change).toHaveBeenCalledTimes(2);
});
it("puts notification preferences and device actions inside Settings",()=>{
 function App(){const[surface,setSurface]=useState<WorkbenchSurface>("SETTINGS");return <UnifiedWorkbench {...base} surface={surface} onSurfaceChange={setSurface} notificationSettings={<div>real notification controls</div>} devicePanel={<button>退出本设备</button>}/>;}
 render(<App/>);fireEvent.click(screen.getByRole("button",{name:"消息通知"}));expect(screen.getByText("real notification controls")).toBeVisible();
 fireEvent.click(screen.getByRole("button",{name:"返回"}));fireEvent.click(screen.getByRole("button",{name:"设备"}));expect(screen.getByRole("button",{name:"退出本设备"})).toBeVisible();
});
it("renames the exact Bridge once without changing its binding or sending",async()=>{
 const rename=vi.fn().mockResolvedValue(undefined),select=vi.fn();
 render(<UnifiedWorkbench {...base} selectedWorkstreamId="exact-a" surface="WORKSPACE" onRenameBridge={rename} onSelectWorkstream={select}/>);
 fireEvent.click(screen.getByRole("button",{name:"Bridge 操作"}));fireEvent.click(screen.getByRole("button",{name:"重命名"}));
 fireEvent.change(screen.getByRole("textbox",{name:"名称"}),{target:{value:"  owner title  "}});
 fireEvent.click(screen.getByRole("button",{name:"保存"}));await waitFor(()=>expect(rename).toHaveBeenCalledExactlyOnceWith("exact-a","owner title"));expect(select).not.toHaveBeenCalled();
});


it("browser Back returns an exact Bridge reader to the directory",async()=>{
 history.replaceState({},"","/mobile");
 function App(){const[surface,setSurface]=useState<WorkbenchSurface>("BRIDGES");const[selected,setSelected]=useState<string>();return <UnifiedWorkbench {...base} surface={surface} onSurfaceChange={setSurface} selectedWorkstreamId={selected} onSelectWorkstream={setSelected} bridgePanel={<div>retained reader</div>}/>;}
 render(<App/>);const list=screen.getByText("最近").parentElement!;fireEvent.click(within(list).getAllByRole("button",{name:/^同名 Bridge/})[0]);
 await waitFor(()=>expect(history.state?.aiwrRoute?.surface).toBe("WORKSPACE"));
 act(()=>history.back());
 await screen.findByRole("heading",{name:/^Bridge$/});
 expect(screen.getByText("retained reader")).not.toBeVisible();
});

it("menu-to-rename waits for the old history layer without closing the new sheet",async()=>{
 history.replaceState({},"","/mobile");
 render(<UnifiedWorkbench {...base} selectedWorkstreamId="exact-a" surface="WORKSPACE" onRenameBridge={vi.fn().mockResolvedValue(undefined)}/>);
 fireEvent.click(screen.getByRole("button",{name:"Bridge 操作"}));
 const menuEntry=history.state.aiwrLayer;
 fireEvent.click(screen.getByRole("button",{name:"重命名"}));
 await waitFor(()=>expect(history.state.aiwrLayer).toBeTruthy());
 await waitFor(()=>expect(history.state.aiwrLayer).not.toBe(menuEntry));
 expect(screen.getByRole("textbox",{name:"名称"})).toBeVisible();
 act(()=>history.back());
 await waitFor(()=>expect(screen.queryByRole("textbox",{name:"名称"})).not.toBeInTheDocument());
});

it("shows the one current side and Bridge review hints without deriving a task from its title",async()=>{
 const bounds=vi.spyOn(Element.prototype,"getBoundingClientRect").mockReturnValue({x:20,y:100,top:100,bottom:218,left:20,right:400,width:380,height:118,toJSON:()=>({})});
 const api=vi.fn().mockResolvedValue([{workstreamId:"exact-a",bindingRevision:1,unreadCount:2,sides:[{role:"DECISION",endpointId:"d",state:"THINKING",checkedAt:Date.now()},{role:"EXECUTION",endpointId:"e",state:"ACTION_REQUIRED",checkedAt:Date.now()}]}]);
 try{render(<UnifiedWorkbench {...base} surface="BRIDGES" bridgeActivityApi={api}/>);expect(await screen.findByText("正在思考")).toBeVisible();expect(screen.queryByText("待你处理")).toBeNull();expect(screen.queryAllByLabelText("有新回复待审阅")).toHaveLength(0);expect(api).toHaveBeenCalledWith(["exact-a","exact-b"]);}finally{bounds.mockRestore();}
});
