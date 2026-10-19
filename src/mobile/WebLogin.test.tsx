import {act,cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {WebLoginGate,WebSessionLogout,type WebLoginApi} from "./WebLogin";
import {readModels} from "../features/workbench/readModelCache";
afterEach(()=>{cleanup();localStorage.removeItem("agbrio.session-hint.v1");});
function fixture(){return {session:vi.fn().mockResolvedValue({authenticated:false}),pair:vi.fn().mockResolvedValue({}),logout:vi.fn().mockResolvedValue({})} satisfies WebLoginApi;}
it("keeps private UI unmounted until a pairing succeeds; remembers once and supports logout",async()=>{
 const api=fixture();render(<WebLoginGate api={api}><div>private original</div><WebSessionLogout/></WebLoginGate>);
 await screen.findByRole("heading",{name:"连接你的工作台"});expect(screen.queryByText("private original")).not.toBeInTheDocument();
 fireEvent.change(await screen.findByLabelText("6 位配对码"),{target:{value:"12a3456"}});
 api.session.mockResolvedValue({authenticated:true,method:"DEVICE"});
 fireEvent.click(screen.getByRole("button",{name:"连接工作台"}));
 await screen.findByText("private original");expect(api.pair).toHaveBeenCalledWith("123456","",true);
 fireEvent.click(screen.getByRole("button",{name:"退出此设备"}));
 await screen.findByRole("heading",{name:"连接你的工作台"});expect(api.logout).toHaveBeenCalledTimes(1);expect(screen.queryByText("private original")).not.toBeInTheDocument();
});
it("shows an actionable expired-code message and never exposes children on rejection",async()=>{
 const api=fixture();api.pair.mockRejectedValue(new Error("WEB_PAIRING_INVALID_OR_EXPIRED"));render(<WebLoginGate api={api}><div>private original</div></WebLoginGate>);
 fireEvent.change(await screen.findByLabelText("6 位配对码"),{target:{value:"123456"}});fireEvent.click(screen.getByRole("button",{name:"连接工作台"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("已过期或已用过");expect(screen.queryByText("private original")).not.toBeInTheDocument();
});
it("returns to login after a protected API reports session revocation",async()=>{
 const api=fixture();api.session.mockResolvedValue({authenticated:true,method:"DEVICE"});render(<WebLoginGate api={api}><div>private original</div></WebLoginGate>);await screen.findByText("private original");
 api.session.mockResolvedValue({authenticated:false});window.dispatchEvent(new Event("aiwr-login-required"));await waitFor(()=>expect(screen.queryByText("private original")).not.toBeInTheDocument());expect(screen.getByRole("heading",{name:"连接你的工作台"})).toBeInTheDocument();
});

it("shows reconnection rather than pairing on initial network failure and recovers without a code",async()=>{
 const api=fixture();api.session.mockRejectedValueOnce(new Error("HTTP 530")).mockResolvedValue({authenticated:true,method:"DEVICE"});render(<WebLoginGate api={api}><div>private original</div></WebLoginGate>);
 await screen.findByRole("heading",{name:"正在重新连接"});expect(screen.queryByLabelText("6 位配对码")).not.toBeInTheDocument();expect(screen.queryByText("private original")).not.toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"重新检查连接"}));await screen.findByText("private original");expect(api.pair).not.toHaveBeenCalled();
});
it("retains mounted work and the device session on a transient connection failure",async()=>{
 const api=fixture();api.session.mockResolvedValueOnce({authenticated:true,method:"DEVICE"}).mockRejectedValue(new Error("Failed to fetch"));render(<WebLoginGate api={api}><textarea aria-label="reply draft" defaultValue="keep this draft"/></WebLoginGate>);
 await screen.findByRole("textbox",{name:"reply draft"});window.dispatchEvent(new Event("online"));await screen.findByRole("status");expect(screen.getByRole("textbox")).toHaveValue("keep this draft");expect(screen.queryByLabelText("6 位配对码")).not.toBeInTheDocument();expect(api.logout).not.toHaveBeenCalled();
});

it("ignores session polling during logout and prevents a late check from mounting old work",async()=>{
 let release!:()=>void;const api=fixture();api.session.mockResolvedValue({authenticated:true,method:"DEVICE"});api.logout.mockImplementation(()=>new Promise<void>(r=>release=r));render(<WebLoginGate api={api}><div>private original</div><WebSessionLogout/></WebLoginGate>);await screen.findByText("private original");fireEvent.click(screen.getByRole("button",{name:"退出此设备"}));window.dispatchEvent(new Event("online"));window.dispatchEvent(new Event("aiwr-login-required"));expect(api.session).toHaveBeenCalledTimes(1);release();await screen.findByRole("heading",{name:"连接你的工作台"});expect(screen.queryByText("private original")).not.toBeInTheDocument();
});

it("confirms a transient 401 once and restores the same mounted draft without pairing",async()=>{
 const api=fixture();api.session.mockResolvedValueOnce({authenticated:true,method:"DEVICE",cacheScope:"same"});render(<WebLoginGate api={api}><textarea aria-label="retained draft" defaultValue="keep me"/></WebLoginGate>);
 const draft=await screen.findByRole("textbox",{name:"retained draft"});let resume!:(value:unknown)=>void;api.session.mockImplementation(()=>new Promise(r=>resume=r));
 act(()=>{for(let n=0;n<6;n++)window.dispatchEvent(new Event("aiwr-login-required"));});expect(draft).not.toBeVisible();expect(api.session).toHaveBeenCalledTimes(2);
 await act(async()=>resume({authenticated:true,method:"DEVICE",cacheScope:"same"}));expect(screen.getByRole("textbox",{name:"retained draft"})).toBe(draft);expect(draft).toHaveValue("keep me");expect(api.pair).not.toHaveBeenCalled();
});
it("an old session's delayed 401 cannot log out the current session",async()=>{
 const api=fixture();api.session.mockResolvedValue({authenticated:true,method:"DEVICE",cacheScope:"current"});render(<WebLoginGate api={api}><div>current private page</div></WebLoginGate>);await screen.findByText("current private page");
 act(()=>window.dispatchEvent(new CustomEvent("aiwr-login-required",{detail:{generation:readModels.generation()-1}})));expect(api.session).toHaveBeenCalledTimes(1);expect(screen.getByText("current private page")).toBeVisible();
});

it("renders the cached page from the last live session before the live check returns, and holds writes until it does",async()=>{
 const {awaitLiveSession,markSessionLive}=await import("./startup");markSessionLive(false);
 localStorage.setItem("agbrio.session-hint.v1",JSON.stringify({method:"DEVICE",cacheScope:"cached",at:Date.now()}));
 let resume!:(value:unknown)=>void;const api=fixture();api.session.mockImplementation(()=>new Promise(r=>resume=r));
 render(<WebLoginGate api={api}><div>cached private page</div></WebLoginGate>);
 expect(screen.getByText("cached private page")).toBeVisible();
 let written=false;const write=awaitLiveSession().then(()=>{written=true;});await Promise.resolve();expect(written).toBe(false);
 await act(async()=>resume({authenticated:true,method:"DEVICE",cacheScope:"cached"}));await write;expect(written).toBe(true);
 expect(screen.getByText("cached private page")).toBeVisible();expect(api.pair).not.toHaveBeenCalled();
});
it("drops the cached page and the hint when the live check says the device is no longer signed in",async()=>{
 const {markSessionLive}=await import("./startup");markSessionLive(false);
 localStorage.setItem("agbrio.session-hint.v1",JSON.stringify({method:"DEVICE",cacheScope:"revoked",at:Date.now()}));
 const api=fixture();render(<WebLoginGate api={api}><div>cached private page</div></WebLoginGate>);
 await screen.findByRole("heading",{name:"连接你的工作台"});expect(screen.queryByText("cached private page")).not.toBeInTheDocument();
 expect(localStorage.getItem("agbrio.session-hint.v1")).toBeNull();
});
it("ignores an expired or malformed hint",async()=>{
 const {readSessionHint}=await import("./startup");
 localStorage.setItem("agbrio.session-hint.v1",JSON.stringify({method:"DEVICE",cacheScope:"old",at:Date.now()-8*24*3600*1000}));expect(readSessionHint()).toBeNull();
 localStorage.setItem("agbrio.session-hint.v1","{bad");expect(readSessionHint()).toBeNull();
 localStorage.setItem("agbrio.session-hint.v1",JSON.stringify({method:"DEVICE",at:Date.now()}));expect(readSessionHint()).toBeNull();
});
