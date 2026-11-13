import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {invoke} from "@tauri-apps/api/core";
import {AssistantSettings,mobileAssistantSettingsApi} from "./AssistantSettings";
vi.mock('@tauri-apps/api/core',async importOriginal=>({...(await importOriginal<typeof import('@tauri-apps/api/core')>()),invoke:vi.fn()}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
it('the desktop API keeps optional native setup available without making it a prerequisite for MCP',async()=>{
 vi.mocked(invoke).mockImplementation(async command=>{
  if(command==='assistant_settings')return {mcpUrl:null,bridges:[],grants:[]} as never;
  if(command==='claude_code_connect')return {phase:'WAITING_SESSION',writesEnabled:false} as never;
  throw Error('Unexpected mutation '+command);
 });
 render(<AssistantSettings/>);fireEvent.click(await screen.findByRole('button',{name:'Claude Code'}));

 const install=screen.getByRole('button',{name:'安装 Claude Code 插件'});expect(install).toBeVisible();expect(screen.getByText(/首次使用不用预装插件/)).toBeVisible();
 expect(vi.mocked(invoke).mock.calls.filter(c=>c[0]==='claude_code_connect')).toHaveLength(0);
 fireEvent.click(install);await waitFor(()=>expect(invoke).toHaveBeenCalledWith('claude_code_connect'));
 expect(await screen.findByText(/安装与配置已完成/)).toHaveTextContent('/reload-plugins');
 expect(vi.mocked(invoke).mock.calls.some(c=>c[0]==='assistant_connect_instance')).toBe(false);
});
it('shows the expired local credential and persists the next duration without reconnecting',async()=>{
 let days=365;
 vi.mocked(invoke).mockImplementation(async (command,args)=>{
  if(command==='assistant_settings')return {mcpUrl:null,bridges:[],grants:[],claudeConnectionPolicy:{days,options:[30,90,180,365,730],latestExpiresAt:Date.now()-86400000}} as never;
  if(command==='claude_code_set_connection_days'){days=(args as {days:number}).days;return undefined as never;}
  throw Error('Unexpected mutation '+command);
 });
 render(<AssistantSettings/>);fireEvent.click(await screen.findByRole('button',{name:'Claude Code'}));
 expect(screen.getByText(/最近的授权已于/)).toBeVisible();
 expect(screen.getByRole('button',{name:'重新连接 Claude Code 插件'})).toBeEnabled();
 const duration=screen.getByRole('combobox',{name:'本机插件授权有效期'});
 expect(duration).toHaveValue('365');
 fireEvent.change(duration,{target:{value:'730'}});
 await waitFor(()=>expect(duration).toHaveValue('730'));
 expect(invoke).toHaveBeenCalledWith('claude_code_set_connection_days',{days:730});
 expect(invoke).not.toHaveBeenCalledWith('claude_code_connect');
});
it('the web/phone API has no native installation or configuration function',()=>{
 expect(mobileAssistantSettingsApi.claudeConnect).toBeUndefined();
});
