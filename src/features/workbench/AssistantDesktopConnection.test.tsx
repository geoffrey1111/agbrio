import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {invoke} from "@tauri-apps/api/core";
import {AssistantSettings,mobileAssistantSettingsApi} from "./AssistantSettings";
vi.mock('@tauri-apps/api/core',async importOriginal=>({...(await importOriginal<typeof import('@tauri-apps/api/core')>()),invoke:vi.fn()}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
it('the actual default desktop API exposes first install with no plugin or grant prerequisite',async()=>{
 vi.mocked(invoke).mockImplementation(async command=>{
  if(command==='assistant_settings')return {mcpUrl:null,bridges:[],grants:[]} as never;
  if(command==='claude_code_connect')return {phase:'WAITING_SESSION',writesEnabled:false} as never;
  throw Error('Unexpected mutation '+command);
 });
 render(<AssistantSettings/>);fireEvent.click(await screen.findByRole('button',{name:'Claude Code'}));
 const install=screen.getByRole('button',{name:'安装并连接 Claude 完整功能'});expect(install).toBeVisible();expect(screen.getByText(/首次使用不用预装插件/)).toBeVisible();
 expect(vi.mocked(invoke).mock.calls.filter(c=>c[0]==='claude_code_connect')).toHaveLength(0);
 fireEvent.click(install);await waitFor(()=>expect(invoke).toHaveBeenCalledWith('claude_code_connect'));
 expect(await screen.findByRole('status')).toHaveTextContent('安装与配置已完成');expect(screen.getByRole('status')).toHaveTextContent('/reload-plugins');
 expect(vi.mocked(invoke).mock.calls.some(c=>c[0]==='assistant_connect_instance')).toBe(false);
});
it('the web/phone API has no native installation or configuration function',()=>{
 expect(mobileAssistantSettingsApi.claudeConnect).toBeUndefined();
});
