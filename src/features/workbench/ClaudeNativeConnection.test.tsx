import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {ClaudeNativeConnection} from "./ClaudeNativeConnection";
import {AssistantConnectionGuide} from "./AssistantConnectionGuide";
import type {ClaudeConnectionResult} from "./ClaudeSessionPicker";
import {claudeCompletionInstructions,claudeManualInstallPowerShell} from './claudeConnectionRecovery';
afterEach(cleanup);
it('keeps phone/web read-only and points at the desktop without an administrative action',()=>{
 render(<ClaudeNativeConnection/>);expect(screen.getByText(/网页和手机不能安装本机组件/)).toBeVisible();expect(screen.queryByRole('button')).toBeNull();
});
it('serializes explicit configuration and distinguishes prepared from loaded',async()=>{
 let finish!:(s:ClaudeConnectionResult)=>void;const connect=vi.fn(()=>new Promise<ClaudeConnectionResult>(r=>{finish=r;}));render(<ClaudeNativeConnection connect={connect}/>);
 const install=screen.getByRole('button',{name:'安装并连接 Claude 完整功能'});fireEvent.click(install);fireEvent.click(install);expect(connect).toHaveBeenCalledTimes(1);
 finish({phase:'WAITING_SESSION',writesEnabled:false});expect(await screen.findByRole('status')).toHaveTextContent('/reload-plugins');expect(screen.getByRole('status')).not.toHaveTextContent('已收到 Claude 连接心跳');
});
it('a live connection does not claim any unregistered task notice',async()=>{
 render(<ClaudeNativeConnection connect={async()=>({phase:'CONNECTED',writesEnabled:false})}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('status')).toHaveTextContent('还需在管理会话登记');expect(screen.getByRole('button',{name:'复制给管理助手的后续指令'})).toBeVisible();
});
it('explains a failed version update without showing a connected state',async()=>{
 render(<ClaudeNativeConnection connect={vi.fn().mockRejectedValue('CLAUDE_PLUGIN_UPDATE_REQUIRED')}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('插件仍是旧版');expect(screen.queryByRole('status')).toBeNull();
});
it('does not mistake an undiscovered engine for an old Claude installation',async()=>{
 render(<ClaudeNativeConnection connect={vi.fn().mockRejectedValue('CLAUDE_DESKTOP_ENGINE_NOT_FOUND')}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('/status');expect(screen.getByRole('alert')).not.toHaveTextContent('更新');expect(screen.queryByRole('status')).toBeNull();
});
it('only asks for a Desktop update when an old engine was actually found',async()=>{
 render(<ClaudeNativeConnection connect={vi.fn().mockRejectedValue('CLAUDE_DESKTOP_ENGINE_TOO_OLD')}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('低于 2.1.286');expect(screen.queryByRole('status')).toBeNull();
});
it('offers the full manual installer after a failed command, without executing it or leaking native text',async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText}});
 const connect=vi.fn().mockRejectedValue('CLAUDE_PLUGIN_ADMIN_REFUSED:INSTALL:1');render(<ClaudeNativeConnection connect={connect}/>);
 fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));expect(await screen.findByRole('alert')).toHaveTextContent('命令未完成');
 expect(screen.getByText(/失败步骤/)).toHaveTextContent('安装插件');expect(screen.queryByRole('status')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'复制手动安装命令'}));await waitFor(()=>expect(writeText).toHaveBeenCalledWith(claudeManualInstallPowerShell));expect(connect).toHaveBeenCalledTimes(1);
});
it('shows idle reload and saved-work restart after configuration, then copies the notification handoff',async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText}});
 render(<ClaudeNativeConnection connect={async()=>({phase:'WAITING_SESSION',writesEnabled:false})}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('status')).toHaveTextContent('尚待当前会话加载');expect(screen.getByText(/仍未连接？先保存工作/)).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'复制重载命令'}));await waitFor(()=>expect(writeText).toHaveBeenCalledWith('/reload-plugins'));
 fireEvent.click(screen.getByRole('button',{name:'复制给管理助手的后续指令'}));await waitFor(()=>expect(writeText).toHaveBeenCalledWith(claudeCompletionInstructions('zh-CN')));
});
it('does not suggest overwriting a different host by reinstalling',async()=>{
 render(<ClaudeNativeConnection connect={vi.fn().mockRejectedValue('CLAUDE_PLUGIN_DIFFERENT_HOST')}/>);fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('另一个 Agbrio');expect(screen.queryByRole('button',{name:'复制手动安装命令'})).toBeNull();
});
it('selection and copy never invoke native configuration; only its explicit button does',async()=>{
 const connect=vi.fn().mockResolvedValue({phase:'WAITING_SESSION',writesEnabled:false});const add=vi.fn();render(<AssistantConnectionGuide mcpUrl="https://fixture.invalid/mcp" grants={[]} onAddGrant={add} onRouteChange={vi.fn()} openHelp={vi.fn()} connectClaude={connect}/>);
 fireEvent.click(screen.getByRole('button',{name:'Claude Code'}));expect(connect).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'安装并连接 Claude 完整功能'}));await waitFor(()=>expect(connect).toHaveBeenCalledTimes(1));expect(add).not.toHaveBeenCalled();
});
