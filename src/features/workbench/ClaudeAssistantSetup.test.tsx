import {cleanup,fireEvent,render,screen,within} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {AssistantSettings,type AssistantSettingsApi} from './AssistantSettings';
afterEach(cleanup);
it('keeps Desktop Code setup in the target chat and never opens web connector settings',async()=>{
 const copied=vi.fn().mockResolvedValue(undefined),openHelp=vi.fn();Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:copied}});
 const api:AssistantSettingsApi={view:async()=>({mcpUrl:'https://user.fixture.invalid/mcp',bridges:[],grants:[]}),create:vi.fn(),revoke:vi.fn(),openHelp};render(<AssistantSettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'Claude Code'}));fireEvent.click(screen.getByRole('navigation',{name:'Claude 连接步骤'}).querySelectorAll('button')[1]);
 expect(screen.queryByRole('button',{name:'打开 Claude 连接器设置'})).toBeNull();fireEvent.click(screen.getByRole('button',{name:'复制给 Code 对话'}));await vi.waitFor(()=>expect(copied).toHaveBeenCalledOnce());const text=copied.mock.calls[0][0];expect(text).toContain('https://user.fixture.invalid/mcp');expect(text).toContain('agbrio_read_app');expect(text).toContain('能正常调用就直接复用');expect(text).not.toContain('Add custom connector');expect(openHelp).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'查看验证方法'}));expect(screen.getByText(/把指令粘贴到这条对话的消息输入框，再点击发送/)).toBeVisible();expect(api.create).not.toHaveBeenCalled();
});
it('completes inline grant preparation before opening the selected desktop connector flow, without installing or sending',async()=>{
 let granted=false;const connect=vi.fn(),openHelp=vi.fn().mockResolvedValue(undefined);const api:AssistantSettingsApi={view:async()=>({mcpUrl:'https://user.fixture.invalid/mcp',bridges:[],grants:granted?[{id:'g',scope:'INSTANCE',approvalMode:'CONVERSATION_REVIEW',label:'Claude Code',workstreamId:'',sourceRole:'BOTH',rules:[],expiresAt:Date.now()+100000,revokedAt:null}]:[]}),create:vi.fn(async()=>{granted=true;}),revoke:vi.fn(),claudeConnect:connect,openHelp};
 render(<AssistantSettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'Claude Code'}));expect(screen.getByRole('button',{name:'去 Claude 添加连接'})).toBeDisabled();fireEvent.click(screen.getByRole('button',{name:'创建 30 天授权'}));
 const form=screen.getByRole('form',{name:'助手授权'});expect(form.closest('.assistant-setup-panel')).not.toBeNull();expect(screen.getByLabelText('助手名称')).toHaveValue('Claude Code');fireEvent.click(within(form).getByRole('button',{name:'保存授权'}));
 expect(await screen.findByText('已有有效授权，可直接复用')).toBeVisible();expect(api.create).toHaveBeenCalledOnce();fireEvent.click(screen.getByRole('button',{name:'去 Claude 添加连接'}));
 expect(screen.getByRole('button',{name:'Claude 桌面版 Code'})).toHaveAttribute('aria-pressed','true');expect(screen.queryByText('claude mcp get agbrio')).toBeNull();expect(connect).not.toHaveBeenCalled();
 expect(openHelp).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'查看验证方法'}));expect(screen.getByRole('button',{name:'复制只读检查指令'})).toBeEnabled();expect(screen.getByText('看到什么才算连上？')).toBeVisible();expect(screen.getByRole('button',{name:'安装 Claude Code 插件'})).toBeVisible();expect(connect).not.toHaveBeenCalled();
});
