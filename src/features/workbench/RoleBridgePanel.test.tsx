import {NativeSurfaceContext} from "./NativeSurfaceContext";
import {clearRoleReviewCache} from "./roleReviewDrafts";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RoleBridgePanel, type RoleBridgeApi, type RoleHandoff, type RoleState } from "./RoleBridgePanel";

afterEach(()=>{cleanup();clearRoleReviewCache();localStorage.clear();vi.useRealTimers();});
const decision = { id: "endpoint-a", provider: "CODEX" as const, externalId: "thread-a", label: "决策对话" };
const execution = { id: "endpoint-b", provider: "CODEX" as const, externalId: "thread-b", label: "执行对话" };
function fixture(explicit = true) {
  const state: RoleState = { bindings: { workstreamId: "work-a", bindingRevision: 4, explicitRoles: explicit, decision: { role: "DECISION", endpoint: decision, cwd: "D:\\decision-project" }, execution: { role: "EXECUTION", endpoint: execution, cwd: "D:\\execution-project" } }, replies: [{ id: "obs-a", endpointId: decision.id, text: "original" }], handoffs: [] };
  const prepared: RoleHandoff = { id: "handoff-a", workstreamId: "work-a", approvedText: "edited", originalText: "original", payloadHash: "hash-1", status: "READY", sourceEndpoint: decision, destinationEndpoint: execution, attachments: [] };
  const api = { state: vi.fn().mockResolvedValue(state), bind: vi.fn().mockResolvedValue(state.bindings), read: vi.fn().mockResolvedValue(state), attachments: vi.fn().mockResolvedValue([]), edit: vi.fn(), prepare: vi.fn().mockResolvedValue(prepared), approve: vi.fn().mockResolvedValue({ ...prepared, status: "APPROVED" }), send: vi.fn().mockResolvedValue({ ...state, handoffs: [{ ...prepared, status: "SENT" }] }), threads: vi.fn().mockResolvedValue({ complete: true, threads: [{ id: "thread-a", label: "决策对话", recencyAt: Date.now()/1000, projectProvenance: "native-test", projectStatus: "PROJECT" as const, projectId: "project-a", projectLabel: "项目 A" }, { id: "thread-b", label: "执行对话", recencyAt: Date.now()/1000, projectProvenance: "native-test", projectStatus: "PROJECT" as const, projectId: "project-b", projectLabel: "项目 B" }] }), connect: vi.fn().mockResolvedValue({}) } satisfies RoleBridgeApi;
  return { state, prepared, api };
}
describe("role-compatible Bridge", () => {
  it("positions the latest message when a previously hidden Bridge becomes visible instead of restoring saved zero",async()=>{
    const{api,state}=fixture();localStorage.setItem("aiwr.role-scroll.work-a",JSON.stringify({DECISION:0,EXECUTION:0}));
    api.state.mockResolvedValue({...state,replies:[{...state.replies[0],id:"old",text:"older reply",observedAt:1000},{...state.replies[0],id:"new",text:"latest reply",observedAt:2000}]});
    const rect=vi.spyOn(Element.prototype,"getBoundingClientRect").mockImplementation(function(this:Element){const top=this.classList.contains("r2-bridge-message")?600-(this.parentElement?.scrollTop??0):100;return {top,bottom:top+300,left:0,right:440,x:0,y:top,width:440,height:300,toJSON:()=>({})};});
    const view=(active:boolean)=><div hidden={!active}><NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={api} readerActive={active}/></NativeSurfaceContext.Provider></div>;
    try{const{rerender}=render(view(false));await screen.findByText("latest reply");expect(document.querySelector(".v4-role-original")?.scrollTop).toBe(0);rerender(view(true));await waitFor(()=>expect(document.querySelector(".v4-role-original")!.scrollTop).toBeGreaterThan(0));expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();}finally{rect.mockRestore();}
  });
  it("positions each role at its latest message on every explicit role entry",async()=>{
    const{api,state}=fixture();api.state.mockResolvedValue({...state,replies:[{...state.replies[0],text:"control latest",observedAt:2000},{id:"exec-old",endpointId:execution.id,text:"execution old",observedAt:500},{id:"exec-new",endpointId:execution.id,text:"execution latest",observedAt:1000}]});
    const rect=vi.spyOn(Element.prototype,"getBoundingClientRect").mockImplementation(function(this:Element){const top=this.classList.contains("r2-bridge-message")?600-(this.parentElement?.scrollTop??0):100;return {top,bottom:top+300,left:0,right:440,x:0,y:top,width:440,height:300,toJSON:()=>({})};});
    try{render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={api}/></NativeSurfaceContext.Provider>);await screen.findByText("control latest");fireEvent.click(screen.getByRole("tab",{name:"执行端 · Codex"}));await screen.findByText("execution latest");await waitFor(()=>expect(document.querySelector(".v4-role-original")!.scrollTop).toBeGreaterThan(0));const reader=document.querySelector<HTMLElement>(".v4-role-original")!;fireEvent.wheel(reader);reader.scrollTop=0;fireEvent.click(screen.getByRole("tab",{name:"执行端 · Codex"}));await waitFor(()=>expect(reader.scrollTop).toBeGreaterThan(0));expect(api.send).not.toHaveBeenCalled();}finally{rect.mockRestore();}
  });

  it("opens the most recently completed side instead of a saved old selection",async()=>{
    const{api,state}=fixture();localStorage.setItem("aiwr.role-selected.work-a","DECISION");api.state.mockResolvedValue({...state,replies:[{...state.replies[0],observedAt:5000,completedAt:1000},{id:"obs-b",endpointId:execution.id,text:"new execution reply",observedAt:3000,completedAt:2000}]});
    render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={api}/></NativeSurfaceContext.Provider>);
    await screen.findByText("new execution reply");expect(screen.getByRole("tab",{name:"执行端 · Codex"})).toHaveAttribute("aria-selected","true");expect(screen.getByText("最新回复")).toBeVisible();
    fireEvent.click(screen.getByRole("tab",{name:"控制端 · Codex"}));expect(await screen.findByText("original")).toBeVisible();
  });
  it("one final confirmation cannot dispatch twice while approval is pending",async()=>{
    const {api,prepared}=fixture();api.prepare.mockResolvedValue({...prepared,approvedText:"original"});
    let accept:(value:RoleHandoff)=>void=()=>{};api.approve.mockImplementation(()=>new Promise(resolve=>{accept=resolve;}));
    render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    const send=await screen.findByRole("button",{name:"确认并发送给执行端"});fireEvent.click(send);fireEvent.click(send);
    await waitFor(()=>expect(api.approve).toHaveBeenCalledTimes(1));expect(api.send).not.toHaveBeenCalled();
    accept({...prepared,status:"APPROVED"});await waitFor(()=>expect(api.send).toHaveBeenCalledTimes(1));
  });
  it("switching Bridge during preparation never approves or sends the stale result",async()=>{
    const {api,prepared,state}=fixture();let accept:(value:RoleHandoff)=>void=()=>{};
    api.prepare.mockImplementation(()=>new Promise(resolve=>{accept=resolve;}));
    const {rerender}=render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    fireEvent.click(await screen.findByRole("button",{name:"确认并发送给执行端"}));await waitFor(()=>expect(api.prepare).toHaveBeenCalledTimes(1));
    api.state.mockResolvedValue({...state,bindings:{...state.bindings,workstreamId:"work-b"}});rerender(<RoleBridgePanel workstreamId="work-b" api={api}/>);
    await act(async()=>accept({...prepared,approvedText:"original"}));expect(api.approve).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
  });
  it("preserves an approved unsent review after Desktop ownership failure and opens connection settings without retry", async () => {
    const {api,state,prepared}=fixture();const open=vi.fn();api.prepare.mockResolvedValue({...prepared,approvedText:"original"});api.state.mockResolvedValue({...state,handoffs:[{...prepared,approvedText:"original",status:"APPROVED"}]});api.send.mockRejectedValue(new Error("MobileApiError: Codex thread is currently owned by another application. Close/release it there, then retry. (Router HTTP 400)"));
    render(<RoleBridgePanel workstreamId="work-a" api={api} onOpenConnection={open}/>);
    fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    fireEvent.click(await screen.findByRole("button",{name:"确认并发送给执行端"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("已批准内容保留，尚未发送");
    fireEvent.click(screen.getByRole("button",{name:"连接设置"}));expect(open).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button",{name:"继续审阅交接"}));
    await waitFor(()=>expect(screen.getByRole("button",{name:"确认并发送给执行端"})).toBeEnabled());
    expect(api.send).toHaveBeenCalledTimes(1);expect(api.approve).toHaveBeenCalledTimes(1);
  });

  it("retains edited unsent content when review closes and resumes without preparing or sending", async () => {
    const {api}=fixture();render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
    fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    fireEvent.change(await screen.findByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"unsent owner edit"}});
    fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button",{name:"对话设置"})).toBeEnabled();
    expect(screen.getByRole("button",{name:"对话设置"})).toBeEnabled();
    fireEvent.click(screen.getByRole("button",{name:"继续审阅交接"}));
    expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("unsent owner edit");
    expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();expect(api.attachments).toHaveBeenCalledTimes(1);
    await waitFor(()=>expect(screen.getByRole("button",{name:"草稿操作"})).toBeEnabled());
    fireEvent.click(screen.getByRole("button",{name:"草稿操作"}));
    fireEvent.click(screen.getByRole("button",{name:"放弃本地编辑"}));
    expect(screen.getByRole("button",{name:"转给执行端"})).toBeEnabled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("reads one complete original at a time and keyboard switching has no transport side effects", async () => {
    const { state, api } = fixture();
    api.state.mockResolvedValue({ ...state, replies: [...state.replies, { id: "obs-b", endpointId: execution.id, text: "execution full original" }] });
    render(<RoleBridgePanel workstreamId="work-a" api={api} />);
    await screen.findByText("execution full original");
    expect(screen.queryByText("original")).not.toBeInTheDocument();
    const tab = screen.getByRole("tab", { name: "执行端 · Codex" });
    fireEvent.keyDown(tab, { key: "Home" });
    expect(screen.getByText("original")).toBeInTheDocument();
    expect(screen.queryByText("execution full original")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "控制端 · Codex" })).toHaveFocus();
    expect(api.read).not.toHaveBeenCalled(); expect(api.prepare).not.toHaveBeenCalled(); expect(api.send).not.toHaveBeenCalled();
  });
  it("chooses two exact Codex identities from different project candidates; binding never sends", async () => {
    const { api } = fixture(false); render(<RoleBridgePanel workstreamId="work-a" api={api} />);
    fireEvent.click(await screen.findByRole("button", { name: /绑定控制端与执行端/ }));
    await waitFor(() => expect(api.threads).toHaveBeenCalledWith("work-a"));
    fireEvent.click(screen.getByRole("button",{name:"选择控制端"}));
    fireEvent.change(screen.getByRole("combobox", { name: "控制端 Codex 对话" }), { target: { value: "thread-a" } });

    fireEvent.click(screen.getByRole("button",{name:"使用此对话"}));
    fireEvent.click(screen.getByRole("button",{name:"选择执行端"}));
    fireEvent.change(screen.getByRole("combobox", { name: "执行端 Codex 对话" }), { target: { value: "thread-b" } });

    expect(api.bind).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button",{name:"使用此对话"}));
    fireEvent.click(screen.getByRole("button", { name: "保存两端" }));
    await waitFor(() => expect(api.bind).toHaveBeenCalledWith("work-a", 4, { provider: "CODEX", externalId: "thread-a", label: "决策对话" }, { provider: "CODEX", externalId: "thread-b", label: "执行对话" }));
    expect(api.send).not.toHaveBeenCalled();
  });
  it("changes one end, retains the other, and swaps only the local review before save", async () => {
    const {api}=fixture(); render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
    fireEvent.click(await screen.findByRole("button", {name:"对话设置"}));
    await waitFor(()=>expect(api.threads).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button",{name:"选择控制端"}));
    fireEvent.click(within(screen.getByRole("group",{name:"选择控制端"})).getByRole("button",{name:"ChatGPT"}));
    fireEvent.change(screen.getByRole("textbox",{name:"控制端 ChatGPT 对话链接"}),{target:{value:"https://chatgpt.com/c/new-exact-id"}});
    fireEvent.click(screen.getByRole("button",{name:"使用此对话"}));
    expect(screen.getByRole("button",{name:"选择执行端"})).toHaveTextContent("执行对话");
    fireEvent.click(screen.getByRole("button",{name:"交换两端"}));
    expect(api.bind).not.toHaveBeenCalled(); expect(api.send).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button",{name:"保存两端"}));
    await waitFor(()=>expect(api.bind).toHaveBeenCalledWith("work-a",4,{provider:"CODEX",externalId:"thread-b",label:"执行对话"},{provider:"CHATGPT",externalId:"https://chatgpt.com/c/new-exact-id",label:"ChatGPT 对话"}));
  });
  it("recognizes the same ChatGPT target with URL variants and blocks saving", async()=>{
    const {api}=fixture(false);render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
    fireEvent.click(await screen.findByRole("button",{name:"绑定控制端与执行端"}));
    await waitFor(()=>expect(screen.getByRole("button",{name:"选择控制端"})).toBeEnabled());
    fireEvent.click(screen.getByRole("button",{name:"选择控制端"}));
    fireEvent.click(within(screen.getByRole("group",{name:"选择控制端"})).getByRole("button",{name:"ChatGPT"}));
    fireEvent.change(screen.getByRole("textbox",{name:"控制端 ChatGPT 对话链接"}),{target:{value:"https://chatgpt.com/c/shared-id"}});

    fireEvent.click(screen.getByRole("button",{name:"使用此对话"}));
    fireEvent.click(screen.getByRole("button",{name:"选择执行端"}));
    fireEvent.click(within(screen.getByRole("group",{name:"选择执行端"})).getByRole("button",{name:"ChatGPT"}));
    fireEvent.change(screen.getByRole("textbox",{name:"执行端 ChatGPT 对话链接"}),{target:{value:"https://chatgpt.com/c/shared-id?source=mobile"}});

    fireEvent.click(screen.getByRole("button",{name:"使用此对话"}));
    expect(screen.getByRole("button",{name:"保存两端"})).toBeDisabled();expect(api.bind).not.toHaveBeenCalled();
  });
  it("opens the guided editor from the existing manage entry without a duplicate entry or binding",async()=>{
    const {api}=fixture(false);const {rerender}=render(<RoleBridgePanel workstreamId="work-a" api={api} externalBindingEntry bindingRequest={0}/>);
    await waitFor(()=>expect(api.state).toHaveBeenCalled());
    expect(screen.queryByRole("button",{name:"绑定控制端与执行端"})).not.toBeInTheDocument();
    rerender(<RoleBridgePanel workstreamId="work-a" api={api} externalBindingEntry bindingRequest={1}/>);
    await screen.findByRole("dialog",{name:"绑定两端"});
    expect(screen.getByRole("button",{name:"选择控制端"})).toBeInTheDocument();
    expect(api.bind).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
  });
  it("recovers parent role mode when an explicit manage request retries a failed initial read",async()=>{
    const {api}=fixture();api.state.mockRejectedValueOnce(new Error("transient read failure"));const onModeChange=vi.fn();
    const {rerender}=render(<RoleBridgePanel workstreamId="work-a" api={api} bindingRequest={0} onModeChange={onModeChange}/>);
    await waitFor(()=>expect(api.state).toHaveBeenCalledTimes(1));
    rerender(<RoleBridgePanel workstreamId="work-a" api={api} bindingRequest={1} onModeChange={onModeChange}/>);
    await screen.findByRole("dialog",{name:"绑定两端"});
    expect(onModeChange).toHaveBeenLastCalledWith(true);expect(api.bind).not.toHaveBeenCalled();
  });
  it("prepares explicit attachments, retains their identity through edits and confirms once", async () => {
    const {state,prepared,api}=fixture();
    api.attachments.mockResolvedValue([{id:"file-a",filename:"selected.txt",sha256:"a".repeat(64),size:53}]);
    api.prepare.mockResolvedValue({...prepared,approvedText:"original",attachments:[{id:"stored-a",filename:"selected.txt",sha256:"a".repeat(64),size:53}]});
    api.edit.mockResolvedValue({...prepared,payloadHash:"hash-2"});
    render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
    fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    const checkbox=await screen.findByRole("checkbox",{name:"selected.txt"});expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);await screen.findByText(/53 字节/);
    expect(api.prepare).toHaveBeenCalledWith("work-a","DECISION","obs-a","original",["file-a"]);
    expect(api.approve).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"edited"}});
    fireEvent.click(screen.getByRole("button",{name:"确认并发送给执行端"}));
    await waitFor(()=>expect(api.send).toHaveBeenCalledTimes(1));
    expect(api.prepare).toHaveBeenCalledTimes(1);expect(api.edit).toHaveBeenCalledWith(prepared.id,"hash-1","edited");expect(api.approve).toHaveBeenCalledWith(prepared.id,"hash-2");
    expect(state.bindings.decision?.cwd).not.toEqual(state.bindings.execution?.cwd);
  });
  it("file selection changes never stack obsolete target directories into the payload",async()=>{
    const {api,prepared,state}=fixture();let latest=prepared,index=0;
    api.attachments.mockResolvedValue([{id:"file-a",filename:"a.md"},{id:"file-b",filename:"b.md"}]);
    api.prepare.mockImplementation(async(_w,_r,_o,body,ids:string[])=>{const id=`prepared-${++index}`;latest={...prepared,id,approvedText:body+`\n\nSelected attachments in the target conversation's project:\n${ids.map(file=>`.aiwr/incoming/${id}/${file}.md`).join("\n")}`,attachments:ids.map(file=>({id:file,filename:file+".md",sha256:"a".repeat(64),size:1}))};return latest;});
    api.edit.mockImplementation(async(id,_hash,text)=>{latest={...latest,id,approvedText:text};return latest;});api.approve.mockImplementation(async()=>({...latest,status:"APPROVED"}));api.send.mockImplementation(async()=>({...state,handoffs:[{...latest,status:"SENT"}]}));
    render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
    fireEvent.click(await screen.findByRole("checkbox",{name:"a.md"}));await waitFor(()=>expect(screen.getByRole("checkbox",{name:"b.md"})).toBeEnabled());
    fireEvent.click(screen.getByRole("checkbox",{name:"b.md"}));await waitFor(()=>expect(api.prepare).toHaveBeenCalledTimes(2));await waitFor(()=>expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toBeEnabled());
    expect(api.prepare.mock.calls[1][3]).toBe("original");expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("original");
    fireEvent.change(screen.getByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"owner edit"}});fireEvent.click(screen.getByRole("button",{name:"确认并发送给执行端"}));
    await waitFor(()=>expect(api.send).toHaveBeenCalledTimes(1));const text=api.edit.mock.calls[0][2];expect(text).toContain("owner edit");expect(text).toContain("prepared-2");expect(text).not.toContain("prepared-1");expect(text.match(/Selected attachments/g)).toHaveLength(1);
    expect(screen.getByRole("dialog").textContent?.match(/Selected attachments/g)).toHaveLength(1);
  });
  it("lost send response disables local resend and retains the original uncertain record", async () => {
    const { state, prepared, api } = fixture();
    api.send.mockImplementation(async () => { api.state.mockResolvedValue({ ...state, handoffs: [{ ...prepared, status: "SENDING", errorMessage: "No resend" }] }); throw new Error("Connection lost"); });
    render(<RoleBridgePanel workstreamId="work-a" api={api} />);
    fireEvent.click(await screen.findByRole("button", { name: "转给执行端" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "跨端交接发送内容" }), { target: { value: "edited" } });
    fireEvent.click(screen.getByRole("button",{name:"确认并发送给执行端"}));
    await screen.findByText(/Connection lost/);
    expect(screen.queryByRole("button",{name:"确认并发送给执行端"})).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button",{name:"检查送达状态"}));
    await waitFor(()=>expect(api.send).toHaveBeenCalledTimes(1));

  });
  it("reload exposes an approved retained handoff without implicitly sending it", async () => {
    const { state, prepared, api } = fixture(); api.state.mockResolvedValue({ ...state, handoffs: [{ ...prepared, status: "APPROVED" }] });
    render(<RoleBridgePanel workstreamId="work-a" api={api} />);
    fireEvent.click(await screen.findByRole("button", { name: "草稿与记录" }));
    fireEvent.click(await screen.findByRole("button", { name: "继续已批准交接" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("edited"); expect(api.send).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "确认并发送给执行端" })).toBeEnabled();
  });
});

it("interrupted latest turn preserves the prior complete reply without a page error or send",async()=>{
 const{api,state}=fixture();api.read.mockResolvedValue({...state,readOutcome:{role:"DECISION",endpointId:state.bindings.decision!.endpoint.id,state:"LATEST_TURN_INTERRUPTED",retainedReply:true}});render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"检查控制端回复"}));expect(await screen.findByRole("status")).toHaveTextContent("控制端最新一轮已中断");expect(screen.getByText("original")).toBeInTheDocument();expect(screen.queryByRole("alert")).not.toBeInTheDocument();expect(api.send).not.toHaveBeenCalled();expect(api.prepare).not.toHaveBeenCalled();
});
it("older interrupted HTTP errors are localized while retaining the visible original",async()=>{
 const{api}=fixture();api.read.mockRejectedValue(Error("MobileApiError: LATEST_TURN_INTERRUPTED (Router HTTP 400)"));render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"检查控制端回复"}));expect(await screen.findByRole("alert")).toHaveTextContent("最新执行状态需核对，目前没有新的完整回复。");expect(screen.getByText("original")).toBeInTheDocument();expect(screen.queryByText(/MobileApiError/)).not.toBeInTheDocument();expect(api.send).not.toHaveBeenCalled();
});

function runningFixture(){const f=fixture();const activity={role:"DECISION" as const,endpointId:decision.id,state:"RUNNING",checkedAt:Date.now(),turnId:"new-turn",resultObservationId:null};const api:RoleBridgeApi={...f.api,sync:vi.fn().mockResolvedValue({...f.state,activities:[activity],snapshotAt:Date.now()})};return{...f,api,activity};}
it("auto-sync distinguishes a running manual turn from the preserved prior result",async()=>{
 const{api}=runningFixture();render(<RoleBridgePanel workstreamId="work-a" api={api}/>);expect(await screen.findByText(/执行中/)).toBeInTheDocument();expect(screen.getByText("已保存")).toBeInTheDocument();expect(screen.getByRole("button",{name:"转给执行端"})).toBeInTheDocument();expect(screen.queryByText(/当前执行已完成/)).not.toBeInTheDocument();expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
});
it("a blocked poll cannot keep displaying completion after its freshness expires",async()=>{
 vi.useFakeTimers();const{api,state,activity}=runningFixture();const sync=vi.fn().mockResolvedValueOnce({...state,activities:[{...activity,state:"COMPLETE",resultObservationId:"obs-a"}],snapshotAt:Date.now()}).mockImplementation(()=>new Promise(()=>{}));api.sync=sync;render(<RoleBridgePanel workstreamId="work-a" api={api}/>);await act(async()=>{});await act(async()=>vi.advanceTimersByTime(2000));await act(async()=>{});expect(screen.getByText("已完成")).toBeInTheDocument();await act(async()=>vi.advanceTimersByTime(8000));expect(screen.getByText("状态待确认")).toBeInTheDocument();expect(screen.queryByText(/当前执行已完成/)).not.toBeInTheDocument();expect(sync).toHaveBeenCalledTimes(2);
});
it("automatic status checks preserve an edited handoff and never send",async()=>{
 const{api,activity,state}=runningFixture();render(<RoleBridgePanel workstreamId="work-a" api={api}/>);await screen.findByText(/执行中/);fireEvent.click(screen.getByRole("button",{name:"转给执行端"}));fireEvent.change(await screen.findByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"my retained review"}});vi.mocked(api.sync!).mockResolvedValue({...state,replies:[{id:"new-result",endpointId:decision.id,text:"new complete result"}],activities:[{...activity,state:"COMPLETE",resultObservationId:"new-result"}],snapshotAt:Date.now()});await act(async()=>window.dispatchEvent(new Event("online")));await waitFor(()=>expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("my retained review"));expect(api.send).not.toHaveBeenCalled();
});
it("closing a saved draft allows the readers to observe the next exact reply",async()=>{
 const {api,state}=fixture();const next={...state,replies:[{...state.replies[0],id:"new-source",text:"next complete reply",observedAt:Date.now()}]};
 const withSync:RoleBridgeApi={...api,sync:vi.fn().mockResolvedValue(state)};
 render(<RoleBridgePanel workstreamId="work-a" api={withSync}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
 fireEvent.change(await screen.findByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"saved old draft"}});fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));
 vi.mocked(withSync.sync!).mockResolvedValue(next);fireEvent(window,new Event("online"));await screen.findByText("next complete reply");
 fireEvent.click(screen.getByRole("button",{name:"转给执行端"}));expect(await screen.findByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("next complete reply");expect(api.send).not.toHaveBeenCalled();
});

it("selects whole complete blocks, preserves edits and sends once after the final confirmation",async()=>{
 const{api,state,prepared}=fixture();const original="结论汇报。\n\n可直接发给原对话：\n\n```text\n请继续当前任务。\n```\n\n其他说明。";
 api.state.mockResolvedValue({...state,replies:[{...state.replies[0],text:original}]});
 const blocks=[{id:"report",kind:"PROSE" as const,recommended:false,text:"结论汇报。"},{id:"command",kind:"INSTRUCTION" as const,recommended:true,text:"请继续当前任务。"},{id:"tail",kind:"PROSE" as const,recommended:false,text:"其他说明。"}];
 const withBlocks:RoleBridgeApi={...api,blocks:vi.fn().mockResolvedValue(blocks)};
 api.prepare.mockImplementation(async(_w,_r,_o,text)=>({...prepared,approvedText:text}));api.approve.mockImplementation(async()=>({...prepared,approvedText:"请继续当前任务。\n我的补充",status:"APPROVED"}));
 render(<RoleBridgePanel workstreamId="work-a" api={withBlocks}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));
 await screen.findByRole("group",{name:"第 2 段 · 推荐指令"});expect(screen.queryByRole("textbox",{name:"跨端交接发送内容"})).not.toBeInTheDocument();expect(screen.getByRole("button",{name:"确认并发送给执行端"})).toBeDisabled();
 expect(withBlocks.blocks).toHaveBeenCalledWith("work-a","DECISION","obs-a");expect(screen.queryByText("展开完整段落")).not.toBeInTheDocument();
 fireEvent.click(within(screen.getByRole("group",{name:"第 2 段 · 推荐指令"})).getByText("请继续当前任务。",{exact:true}));expect(screen.getByRole("checkbox",{name:"第 2 段 · 推荐指令"})).toBeChecked();expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"编辑发送内容"}));const editor=screen.getByRole("textbox",{name:"跨端交接发送内容"});expect(editor).toHaveValue("请继续当前任务。");fireEvent.change(editor,{target:{value:"请继续当前任务。\n我的补充"}});
 fireEvent.click(screen.getByRole("button",{name:"选择段落"}));fireEvent.click(screen.getByRole("button",{name:"编辑发送内容"}));expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("请继续当前任务。\n我的补充");
 fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));fireEvent.click(screen.getByRole("button",{name:"继续审阅交接"}));fireEvent.click(await screen.findByRole("button",{name:"编辑发送内容"}));expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("请继续当前任务。\n我的补充");expect(withBlocks.blocks).toHaveBeenCalledTimes(1);
 fireEvent.click(screen.getByRole("button",{name:"确认并发送给执行端"}));await waitFor(()=>expect(api.send).toHaveBeenCalledTimes(1));expect(api.prepare).toHaveBeenCalledWith("work-a","DECISION","obs-a","请继续当前任务。\n我的补充",[]);expect(api.approve).toHaveBeenCalledTimes(1);

});
it("combines whole blocks in source order and selecting all restores exact full original",async()=>{
 const{api,state}=fixture();const blocks=[{id:"a",kind:"PROSE" as const,recommended:false,text:"report"},{id:"b",kind:"INSTRUCTION" as const,recommended:true,text:"command"},{id:"c",kind:"CODE" as const,recommended:false,text:"example"}];const withBlocks:RoleBridgeApi={...api,blocks:vi.fn().mockResolvedValue(blocks)};
 render(<RoleBridgePanel workstreamId="work-a" api={withBlocks}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));await screen.findByRole("checkbox",{name:"第 2 段 · 推荐指令"});
 fireEvent.click(screen.getByText("command",{exact:true}));fireEvent.click(screen.getByText("report",{exact:true}));fireEvent.click(screen.getByRole("button",{name:"编辑发送内容"}));expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("report\n\ncommand");
 fireEvent.click(screen.getByRole("button",{name:"选择段落"}));fireEvent.click(screen.getByRole("checkbox",{name:"第 3 段"}));fireEvent.click(screen.getByRole("button",{name:"编辑发送内容"}));expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue(state.replies[0].text);expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
});

it("opens the execution reply independently while preserving the older decision draft",async()=>{
 const{api,state}=fixture();api.state.mockResolvedValue({...state,replies:[...state.replies,{id:"execution-new",endpointId:execution.id,text:"new execution result",observedAt:Date.now()-1000}]});render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
 fireEvent.click(await screen.findByRole("tab",{name:"控制端 · Codex"}));fireEvent.click(screen.getByRole("button",{name:"转给执行端"}));fireEvent.change(await screen.findByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"decision draft retained"}});fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));
 fireEvent.click(screen.getByRole("tab",{name:"执行端 · Codex"}));fireEvent.click(screen.getByRole("button",{name:"转给控制端"}));expect(await screen.findByRole("dialog",{name:"转发"})).toBeInTheDocument();expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("new execution result");fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));
 fireEvent.click(screen.getByRole("tab",{name:"控制端 · Codex"}));fireEvent.click(screen.getByRole("button",{name:"继续审阅交接"}));expect(await screen.findByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("decision draft retained");expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
});
it("keeps each Bridge draft across unmount and never reuses it for a newer source message",async()=>{
 const{api,state}=fixture();const first=render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));fireEvent.change(await screen.findByRole("textbox",{name:"跨端交接发送内容"}),{target:{value:"owner draft"}});first.unmount();
 render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"继续审阅交接"}));expect(await screen.findByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("owner draft");fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));api.read.mockResolvedValue({...state,replies:[{id:"new-observation",endpointId:decision.id,text:"new source",observedAt:Date.now()}]});fireEvent.click(screen.getByRole("button",{name:"检查控制端回复"}));fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));expect(await screen.findByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("new source");expect(api.send).not.toHaveBeenCalled();
});

it("opens direct original-chat replies only after the explicit action and exact native ID",async()=>{
 const{api}=fixture();const watch={threadId:"thread-a",label:"source",cwd:"D:/public-fixture",enabled:true,generation:1,checkedAt:Date.now(),errorCode:null,snapshot:{state:"IDLE",turnId:null,itemId:null,text:""}};
 const extended:RoleBridgeApi={...api,openChat:vi.fn().mockResolvedValue(watch),chat:{state:vi.fn().mockResolvedValue({watch,host:"fixture",ownedTurnId:null,externalBusy:false,replies:[],requests:[],goal:null}),command:vi.fn()}};
 render(<RoleBridgePanel workstreamId="work-a" api={extended}/>);await screen.findByRole("button",{name:"回复原对话"});expect(extended.openChat).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"回复原对话"}));await screen.findByRole("dialog",{name:"原对话回复"});expect(extended.openChat).toHaveBeenCalledWith("thread-a");expect(vi.mocked(extended.chat!.command).mock.calls.filter(([input])=>input.action!=="HISTORY")).toHaveLength(0);expect(api.send).not.toHaveBeenCalled();
});

it("does not label an empty endpoint as a latest-read result",async()=>{
 const {state,api}=fixture();api.state.mockResolvedValue({...state,replies:[]});
 render(<RoleBridgePanel workstreamId="work-a" api={api}/>);
 await screen.findByRole("tab",{name:"控制端 · Codex"});
 expect(screen.queryByText("最新已读取")).toBeNull();
 expect(api.prepare).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
});

it("native manual read never relabels a retained prior result as latest during a new running turn",async()=>{const{api,state,activity}=runningFixture();api.read=vi.fn().mockResolvedValue({...state,activities:[activity],snapshotAt:Date.now()});render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={api}/></NativeSurfaceContext.Provider>);await screen.findByText("执行中 · 上次结果");fireEvent.click(screen.getByRole("button",{name:"检查控制端回复"}));await waitFor(()=>expect(api.read).toHaveBeenCalled());expect(screen.getByText("执行中 · 上次结果")).toBeInTheDocument();expect(screen.queryByText("最新已读取")).not.toBeInTheDocument();expect(api.send).not.toHaveBeenCalled();});


it("refreshes untouched cached segmentation and selects the instruction without its introduction",async()=>{
 const{api,state}=fixture();const original="可直接发给原执行对话：\n\n> 继续11项校准。";
 api.state.mockResolvedValue({...state,replies:[{...state.replies[0],text:original}]});
 const blocks=vi.fn().mockResolvedValueOnce([{id:"old",kind:"PROSE",recommended:false,text:original}]).mockResolvedValue([{id:"intro",kind:"PROSE",recommended:false,text:"可直接发给原执行对话："},{id:"instruction",kind:"INSTRUCTION",recommended:true,text:"继续11项校准。"}]);
 render(<RoleBridgePanel workstreamId="work-a" api={{...api,blocks}}/>);
 fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));await screen.findByRole("group",{name:"第 1 段"});fireEvent.click(screen.getByRole("button",{name:"关闭审阅"}));
 fireEvent.click(screen.getByRole("button",{name:"继续审阅交接"}));const block=await screen.findByRole("group",{name:"第 2 段 · 推荐指令"});
 expect(blocks).toHaveBeenCalledTimes(2);expect(within(block).queryByText("可直接发给原执行对话：")).toBeNull();
 fireEvent.click(within(block).getByRole("checkbox"));fireEvent.click(screen.getByRole("button",{name:"编辑发送内容"}));
 expect(screen.getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("继续11项校准。");expect(api.prepare).not.toHaveBeenCalled();expect(api.approve).not.toHaveBeenCalled();expect(api.send).not.toHaveBeenCalled();
});

it("retains a full earlier deliverable before repeated blocked updates and forwards the explicitly selected source",async()=>{
 const {api,state}=fixture();api.state.mockResolvedValue({...state,replies:[{id:"blocked-2",endpointId:decision.id,text:"Goal is blocked",observedAt:3000},{id:"valuable",endpointId:decision.id,text:"Complete delivery checklist; ready for review",observedAt:1000},{id:"blocked-1",endpointId:decision.id,text:"Still waiting for the owner",observedAt:2000}]});
 render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={api}/></NativeSurfaceContext.Provider>);await screen.findByText("Complete delivery checklist; ready for review");
 const messages=[...document.querySelectorAll(".r2-bridge-message")];expect(messages.map(n=>n.getAttribute("data-observation-id"))).toEqual(["valuable","blocked-1","blocked-2"]);
 expect(screen.queryByRole("button",{name:"选择此条"})).toBeNull();expect(screen.queryByRole("button",{name:"已选此条"})).toBeNull();expect(messages.every(n=>!n.hasAttribute("data-chosen"))).toBe(true);
 fireEvent.click(screen.getByRole("button",{name:"转给执行端"}));const dialog=await screen.findByRole("dialog",{name:"转发"});fireEvent.click(within(dialog).getByRole("button",{name:/来源回复/}));const sources=within(dialog).getByRole("group",{name:"选择要转发的回复"});fireEvent.click(within(sources).getByRole("button",{name:/Complete delivery checklist/}));
 expect(await within(dialog).findByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue("Complete delivery checklist; ready for review");expect(api.attachments).toHaveBeenCalledWith("work-a","DECISION","valuable");expect(api.send).not.toHaveBeenCalled();
 fireEvent.click(within(dialog).getByRole("button",{name:"关闭审阅"}));expect(screen.queryByRole("button",{name:"选择此条"})).toBeNull();expect(screen.queryByRole("button",{name:"已选此条"})).toBeNull();expect(document.querySelector(".r2-bridge-message[data-chosen]")).toBeNull();
});

it("explains a shared Goal prewrite block without replaying or silently changing the Goal",async()=>{
 const {api,prepared}=fixture();api.prepare.mockResolvedValue({...prepared,approvedText:"original"});api.send.mockRejectedValue(Error("SHARED_TARGET_GOAL_NOT_IDLE"));
 render(<RoleBridgePanel workstreamId="work-a" api={api}/>);fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));fireEvent.click(await screen.findByRole("button",{name:"确认并发送给执行端"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("本次未发送");expect(screen.getByRole("alert")).not.toHaveTextContent("SHARED_TARGET_GOAL_NOT_IDLE");expect(api.send).toHaveBeenCalledOnce();
});

it("switches directly from a blocked update to the exact earlier deliverable inside relay, without carrying its selection or attachments",async()=>{
 const{api,state}=fixture();const blocked={id:"blocked",endpointId:decision.id,text:"Goal is blocked",observedAt:3000},valuable={id:"valuable",endpointId:decision.id,text:"Delivery ready for review",observedAt:1000};
 api.state.mockResolvedValue({...state,replies:[blocked,valuable],activities:[{role:"DECISION",endpointId:decision.id,state:"ACTION_REQUIRED",goalStatus:"blocked",checkedAt:Date.now()}]});
 const blocks=vi.fn().mockImplementation(async(_w,_r,id)=>[{id:id+"-body",kind:"PROSE",recommended:false,text:id==="valuable"?valuable.text:blocked.text}]);
 render(<NativeSurfaceContext.Provider value><RoleBridgePanel workstreamId="work-a" api={{...api,blocks}}/></NativeSurfaceContext.Provider>);
 fireEvent.click(await screen.findByRole("button",{name:"转给执行端"}));const dialog=await screen.findByRole("dialog",{name:"转发"});
 const sources=within(dialog).getByRole("group",{name:"选择要转发的回复"});fireEvent.click(within(sources).getByRole("button",{name:/Delivery ready for review/}));
 await waitFor(()=>expect(api.attachments).toHaveBeenCalledWith("work-a","DECISION","valuable"));
 const selected=await within(dialog).findByRole("checkbox",{name:"第 1 段"});expect(selected).not.toBeChecked();fireEvent.click(selected);fireEvent.click(within(dialog).getByRole("button",{name:"编辑发送内容"}));
 expect(within(dialog).getByRole("textbox",{name:"跨端交接发送内容"})).toHaveValue(valuable.text);expect(blocks).toHaveBeenCalledWith("work-a","DECISION","valuable");expect(api.send).not.toHaveBeenCalled();
});
