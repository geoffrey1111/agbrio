// @vitest-environment jsdom
import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {SharedRetirementPrompt} from "./SharedRetirement";
import type {CodexConnectionStatus} from "./CodexConnection";

afterEach(cleanup);
const waiting=(turns:number,desktop:boolean):CodexConnectionStatus=>({enabled:true,ready:false,desktopConnected:false,state:"SHARED_RETIRING",sharedActiveTurns:turns,sharedDesktopConnected:desktop});
const show=(value:CodexConnectionStatus,retire=vi.fn(async()=>{}),native=true)=>{render(<SharedRetirementPrompt status={async()=>value} retire={retire} native={native}/>);return retire;};

it("asks before stopping a backend in use and says what it costs",async()=>{
 const retire=show(waiting(2,true));
 expect(await screen.findByRole("heading",{name:"关闭旧的共享后台？"})).toBeVisible();
 expect(screen.getByText("正在运行的 2 个任务会中断，不会自动重发。")).toBeVisible();
 expect(screen.getByText("正在使用它的 Codex 窗口要退出后重新打开。")).toBeVisible();
 expect(retire).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"关闭共享后台"}));
 await waitFor(()=>expect(retire).toHaveBeenCalledOnce());
 await waitFor(()=>expect(screen.queryByRole("heading",{name:"关闭旧的共享后台？"})).toBeNull());
});
it("does nothing when the owner chooses later",async()=>{
 const retire=show(waiting(0,true));
 fireEvent.click(await screen.findByRole("button",{name:"稍后再说"}));
 await waitFor(()=>expect(screen.queryByRole("heading",{name:"关闭旧的共享后台？"})).toBeNull());
 expect(retire).not.toHaveBeenCalled();
});
it("leaves out the task line when nothing is running",async()=>{
 show(waiting(0,true));
 expect(await screen.findByText("正在使用它的 Codex 窗口要退出后重新打开。")).toBeVisible();
 expect(screen.queryByText(/个任务会中断/)).toBeNull();
});
it("stays open and reports a call that did not start",async()=>{
 show(waiting(1,false),vi.fn(async()=>{throw Error("SHARED_RETIREMENT_BUSY");}));
 fireEvent.click(await screen.findByRole("button",{name:"关闭共享后台"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("这次没有开始关闭");
 expect(screen.getByRole("heading",{name:"关闭旧的共享后台？"})).toBeVisible();
});
it("never asks while nothing is in use, or outside the desktop",async()=>{
 const reads=vi.fn(async()=>waiting(0,false));
 render(<SharedRetirementPrompt status={reads} retire={vi.fn()} native/>);
 await waitFor(()=>expect(reads).toHaveBeenCalled());
 expect(screen.queryByRole("dialog")).toBeNull();
 cleanup();
 const never=vi.fn(async()=>waiting(1,true));
 render(<SharedRetirementPrompt status={never} retire={vi.fn()} native={false}/>);
 expect(never).not.toHaveBeenCalled();
 expect(screen.queryByRole("dialog")).toBeNull();
});
