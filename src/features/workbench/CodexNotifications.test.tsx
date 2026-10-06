import {useState} from "react";
import {act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CodexNotifications, type NotificationApi, type WatchEvent, type CodexWatch } from "./CodexNotifications";
afterEach(cleanup);
const event:WatchEvent={sequence:1,threadId:"native-exact",label:"执行对话",cwd:"D:\\project",snapshot:{state:"RESULT_READY",turnId:"turn-exact",itemId:"item-exact",text:"preview"},observedAt:100};
function fixture(){return {watches:vi.fn().mockResolvedValue([]),connect:vi.fn().mockResolvedValue({}),threads:vi.fn().mockResolvedValue({complete:true,threads:[{id:"native-exact",label:"执行对话",recencyAt:Date.now()/1000,projectProvenance:"项目"}]}),enable:vi.fn().mockResolvedValue({}),pause:vi.fn().mockResolvedValue({}),feed:vi.fn().mockResolvedValue({events:[event],nextCursor:1,hasMore:false}),event:vi.fn().mockResolvedValue({...event,snapshot:{...event.snapshot,text:"完整结果 <script>literal</script>"}}),webUrl:vi.fn().mockResolvedValue("https://router.example/mobile/notifications")} satisfies NotificationApi;}
describe("independent Codex notifications",()=>{
 it("returns focus to the exact invoking notification after reading the original",async()=>{
  const scroll=vi.spyOn(window,"scrollTo").mockImplementation(()=>{});
  const api=fixture();render(<CodexNotifications api={api} standalone/>);
  fireEvent.click(await screen.findByRole("button",{name:"查看完整通知 #1"}));
  await screen.findByText("完整结果 <script>literal</script>");
  fireEvent.click(screen.getByRole("button",{name:"关闭内容"}));
  expect(screen.getByRole("button",{name:"查看完整通知 #1"})).toHaveFocus();
  expect(api.event).toHaveBeenCalledTimes(1);scroll.mockRestore();
 });
 it("starts at recent notifications and keyboard navigation reveals setup separately",async()=>{
  const api=fixture(); render(<CodexNotifications api={api} standalone/>);
  await screen.findByRole("button",{name:"查看完整通知 #1"});
  expect(screen.queryByRole("button",{name:"选择监听对话"})).not.toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("tab",{name:"最近通知"}),{key:"ArrowRight"});
  expect(screen.getByRole("tab",{name:"监听对话"})).toHaveFocus();
  expect(screen.getByRole("button",{name:"选择监听对话"})).toBeInTheDocument();
  expect(api.connect).not.toHaveBeenCalled(); expect(api.enable).not.toHaveBeenCalled();
 });
 it("keeps the newest exact notification when two click requests resolve out of order",async()=>{
  let open!:(sequence:number)=>void;let first!:(value:WatchEvent)=>void;
  const api={...fixture(),onOpen:vi.fn(async(callback:(sequence:number)=>void)=>{open=callback;return()=>{};})};
  api.event.mockImplementation(sequence=>sequence===1?new Promise(resolve=>{first=resolve;}):Promise.resolve({...event,sequence:2,snapshot:{...event.snapshot,text:"newest exact original"}}));
  render(<CodexNotifications api={api}/>);await waitFor(()=>expect(open).toBeDefined());
  open(1);open(2);await screen.findByText("newest exact original");
  first({...event,snapshot:{...event.snapshot,text:"stale original"}});
  await waitFor(()=>expect(screen.queryByText("stale original")).not.toBeInTheDocument());
  expect(screen.getByText("newest exact original")).toBeInTheDocument();
 });
 it("shows the newest twenty with a clear notice for an expired notification link",async()=>{
  const api=fixture();api.feed.mockResolvedValue({events:Array.from({length:25},(_,i)=>({...event,sequence:i+1})),nextCursor:25,hasMore:false});
  api.event.mockRejectedValue("WATCH_EVENT_NOT_FOUND");
  render(<CodexNotifications api={api} standalone/>);
  await screen.findByRole("button",{name:"查看完整通知 #25"});
  expect(screen.queryByRole("button",{name:"查看完整通知 #5"})).not.toBeInTheDocument();
  expect(screen.getAllByRole("button",{name:/查看完整通知/})).toHaveLength(20);
  fireEvent.click(screen.getByRole("button",{name:"查看完整通知 #25"}));
  expect(await screen.findByText("这条通知已清理或不可用。请查看最近通知或对话当前内容。")).toBeInTheDocument();
  expect(screen.queryByText("WATCH_EVENT_NOT_FOUND")).not.toBeInTheDocument();
 });
 it("opens an exact notified sequence without scanning or changing a binding",async()=>{const api={...fixture(),onOpen:vi.fn(async(callback:(seq:number)=>void)=>{callback(1);return()=>{};})};render(<CodexNotifications api={api}/>);expect(await screen.findByText("完整结果 <script>literal</script>")).toBeInTheDocument();expect(api.event).toHaveBeenCalledWith(1);expect(api.threads).not.toHaveBeenCalled();expect(api.enable).not.toHaveBeenCalled();});
 it("uses persisted milliseconds for readable current time and healthy observation",async()=>{const api=fixture();const checkedAt=Date.now();api.watches.mockResolvedValue([{threadId:event.threadId,label:event.label,cwd:event.cwd,enabled:true,generation:1,snapshot:event.snapshot,checkedAt,errorCode:null}]);render(<CodexNotifications api={api} standalone/>);fireEvent.click(screen.getByRole("tab",{name:"监听对话"}));const checked=await screen.findByText(/最后成功检查：/);expect(checked.textContent).toContain(String(new Date().getFullYear()));expect(screen.queryByText("最近未成功检查")).not.toBeInTheDocument();});
 it("subscribes by exact selected native ID without requiring a Bridge",async()=>{const api=fixture();render(<CodexNotifications api={api} standalone/>);fireEvent.click(screen.getByRole("tab",{name:"监听对话"}));fireEvent.click(screen.getByRole("button",{name:"选择监听对话"}));const select=await screen.findByLabelText("本地 Codex 对话");fireEvent.change(select,{target:{value:"native-exact"}});fireEvent.click(screen.getByRole("button",{name:"开始监听所选对话"}));await waitFor(()=>expect(api.enable).toHaveBeenCalledWith("native-exact"));expect(api.pause).not.toHaveBeenCalled();});
 it("fetches the exact original event only after opening it; renders provider text inert",async()=>{const api=fixture();render(<CodexNotifications api={api} standalone/>);fireEvent.click(await screen.findByRole("button",{name:"查看完整通知 #1"}));expect(await screen.findByText("完整结果 <script>literal</script>")).toBeInTheDocument();expect(api.event).toHaveBeenCalledWith(1);expect(api.enable).not.toHaveBeenCalled();});
 it("pauses a subscription without discarding its result",async()=>{const api=fixture();const watch:CodexWatch={threadId:event.threadId,label:event.label,cwd:event.cwd,enabled:true,generation:1,snapshot:event.snapshot,checkedAt:100,errorCode:null};api.watches.mockResolvedValue([watch]);render(<CodexNotifications api={api} standalone/>);fireEvent.click(screen.getByRole("tab",{name:"监听对话"}));fireEvent.click(await screen.findByRole("button",{name:"暂停监听"}));await waitFor(()=>expect(api.pause).toHaveBeenCalledWith(event.threadId));expect(api.enable).not.toHaveBeenCalled();});
});

it("uses the full workspace page for an exact native notification and preserves its list scroll",async()=>{
 let open:(sequence:number)=>void=()=>{};const api={...fixture(),onOpen:vi.fn(async(fn:(sequence:number)=>void)=>{open=fn;return()=>{};})};
 function Harness(){const[active,setActive]=useState(false);return <><CodexNotifications api={api} workbenchPage={{active,open:()=>setActive(true)}}/><section className="v3-workbench-main"/></>;}
 render(<Harness/>);await waitFor(()=>expect(api.onOpen).toHaveBeenCalled());
 act(()=>open(1));await screen.findByText("完整结果 <script>literal</script>");
 expect(screen.getByRole("region",{name:"通知中心"})).toBeVisible();expect(screen.queryByRole("dialog")).toBeNull();
 fireEvent.click(screen.getByRole("button",{name:"关闭内容"}));const entry=await screen.findByRole("button",{name:"查看完整通知 #1"});
 const body=entry.closest<HTMLElement>(".v3-notifications-body")!;body.scrollTop=240;fireEvent.click(entry);
 await screen.findByText("完整结果 <script>literal</script>");fireEvent.click(screen.getByRole("button",{name:"关闭内容"}));
 expect(body.scrollTop).toBe(240);expect(screen.getByRole("button",{name:"查看完整通知 #1"})).toHaveFocus();
 expect(api.enable).not.toHaveBeenCalled();expect(api.threads).not.toHaveBeenCalled();
});


it("browser Back from a pushed exact notification returns to its list and clears the deep link",async()=>{
 history.replaceState({},"","/mobile/notifications?event=1");
 const api=fixture();render(<CodexNotifications api={api} standalone/>);
 await screen.findByText("完整结果 <script>literal</script>");
 await waitFor(()=>expect(history.state?.aiwrLayer).toBeTruthy());
 act(()=>history.back());
 await screen.findByRole("button",{name:"查看完整通知 #1"});
 expect(location.search).not.toContain("event=");expect(api.event).toHaveBeenCalledTimes(1);
});

it('removes an exact notification without another confirmation and restores it through undo',async()=>{
 const api={...fixture(),remove:vi.fn().mockResolvedValue({})};
 render(<CodexNotifications api={api} standalone/>);
 await screen.findByRole('button',{name:'查看完整通知 #1'});
 fireEvent.click(screen.getByRole('button',{name:'删除 通知 #1'}));
 expect(screen.queryByRole('button',{name:'查看完整通知 #1'})).toBeNull();expect(screen.queryByRole('dialog')).toBeNull();
 await waitFor(()=>expect(api.remove).toHaveBeenCalledWith('EVENT','1',true));
 fireEvent.click(screen.getByRole('button',{name:'撤销'}));await screen.findByRole('button',{name:'查看完整通知 #1'});
 await waitFor(()=>expect(api.remove).toHaveBeenLastCalledWith('EVENT','1',false));
 expect(api.enable).not.toHaveBeenCalled();expect(api.pause).not.toHaveBeenCalled();
});
it('removes only a subscription and restores it without sending to the provider',async()=>{
 const watch:CodexWatch={...event,enabled:true,generation:1,checkedAt:1,errorCode:null};
 const api={...fixture(),remove:vi.fn().mockResolvedValue({}),watches:vi.fn().mockResolvedValue([watch])};
 render(<CodexNotifications api={api} standalone/>);
 await screen.findByRole('button',{name:'查看完整通知 #1'});fireEvent.click(screen.getByRole('tab',{name:'监听对话'}));
 fireEvent.click(await screen.findByRole('button',{name:'删除 监听 执行对话'}));
 expect(screen.queryByRole('button',{name:'查看当前内容'})).toBeNull();
 await waitFor(()=>expect(api.remove).toHaveBeenCalledWith('WATCH','native-exact',true));
 fireEvent.click(screen.getByRole('button',{name:'撤销'}));await screen.findByRole('button',{name:'查看当前内容'});
 await waitFor(()=>expect(api.remove).toHaveBeenLastCalledWith('WATCH','native-exact',false));
 expect(api.enable).not.toHaveBeenCalled();expect(api.pause).not.toHaveBeenCalled();
});
