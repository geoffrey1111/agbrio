import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {ClaudeNativeConnection} from "./ClaudeNativeConnection";
import {AssistantConnectionGuide} from "./AssistantConnectionGuide";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
afterEach(cleanup);
it('keeps phone/web read-only and points at the desktop without an administrative action',()=>{
 render(<ClaudeNativeConnection/>);expect(screen.getByText(/请在电脑端 Agbrio/)).toBeVisible();expect(screen.queryByRole('button')).toBeNull();
});
it('serializes explicit configuration and distinguishes prepared from loaded',async()=>{
 let finish!:(s:ClaudeConnectionResult)=>void;const connect=vi.fn(()=>new Promise<ClaudeConnectionResult>(r=>{finish=r;}));render(<ClaudeNativeConnection connect={connect}/>);
 fireEvent.click(screen.getByRole('button'));fireEvent.click(screen.getByRole('button'));expect(connect).toHaveBeenCalledTimes(1);
 finish({phase:'WAITING_SESSION',writesEnabled:false});expect(await screen.findByRole('status')).toHaveTextContent('/reload-plugins');expect(screen.getByRole('status')).not.toHaveTextContent('已收到 Claude 连接心跳');
});
it('a live connection does not claim any unregistered task notice',async()=>{
 render(<ClaudeNativeConnection connect={async()=>({phase:'CONNECTED',writesEnabled:false})}/>);fireEvent.click(screen.getByRole('button'));
 expect(await screen.findByRole('status')).toHaveTextContent('还需在管理会话登记');expect(screen.getByText(/WAITING 回执/)).toBeVisible();
});
it('explains a failed version update without showing a connected state',async()=>{
 render(<ClaudeNativeConnection connect={vi.fn().mockRejectedValue('CLAUDE_PLUGIN_UPDATE_REQUIRED')}/>);fireEvent.click(screen.getByRole('button'));
 expect(await screen.findByRole('alert')).toHaveTextContent('插件仍是旧版');expect(screen.queryByRole('status')).toBeNull();
});
it('selection and copy never invoke native configuration; only its explicit button does',async()=>{
 const connect=vi.fn().mockResolvedValue({phase:'WAITING_SESSION',writesEnabled:false});const add=vi.fn();render(<AssistantConnectionGuide mcpUrl="https://fixture.invalid/mcp" grants={[]} onAddGrant={add} onRouteChange={vi.fn()} openHelp={vi.fn()} connectClaude={connect}/>);
 fireEvent.click(screen.getByRole('button',{name:'Claude Code'}));expect(connect).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'启用 Claude 通知与手机回复'}));await waitFor(()=>expect(connect).toHaveBeenCalledTimes(1));expect(add).not.toHaveBeenCalled();
});
