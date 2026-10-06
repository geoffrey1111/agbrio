import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {WebLoginGate,WebSessionLogout,type WebLoginApi} from "./WebLogin";
afterEach(cleanup);
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
 window.dispatchEvent(new Event("aiwr-login-required"));await waitFor(()=>expect(screen.queryByText("private original")).not.toBeInTheDocument());expect(screen.getByRole("heading",{name:"连接你的工作台"})).toBeInTheDocument();
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
 let release!:()=>void;const api=fixture();api.session.mockResolvedValue({authenticated:true,method:"DEVICE"});api.logout.mockImplementation(()=>new Promise<void>(r=>release=r));render(<WebLoginGate api={api}><div>private original</div><WebSessionLogout/></WebLoginGate>);await screen.findByText("private original");fireEvent.click(screen.getByRole("button",{name:"退出此设备"}));window.dispatchEvent(new Event("online"));expect(api.session).toHaveBeenCalledTimes(1);release();await screen.findByRole("heading",{name:"连接你的工作台"});expect(screen.queryByText("private original")).not.toBeInTheDocument();
});
