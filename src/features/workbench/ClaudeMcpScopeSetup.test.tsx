import {cleanup,fireEvent,render,screen} from '@testing-library/react';import {afterEach,expect,it,vi} from 'vitest';
import {invoke,isTauri} from '@tauri-apps/api/core';import {ClaudeMcpScopeSetup} from './ClaudeMcpScopeSetup';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
afterEach(()=>{cleanup();vi.clearAllMocks();vi.restoreAllMocks();});
it('configures the user-wide server only after an owner click and never claims tool discovery',async()=>{
 vi.spyOn(navigator,'userAgent','get').mockReturnValue('Windows');vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockResolvedValue({phase:'CONFIGURED',targetSessionVerified:false});
 render(<ClaudeMcpScopeSetup/>);expect(invoke).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'为这台电脑配置 MCP'}));
 expect(await screen.findByText('用户级配置已就绪，适用于不同项目。')).toBeVisible();expect(invoke).toHaveBeenCalledExactlyOnceWith('claude_code_configure_mcp');expect(screen.queryByText('连接成功')).toBeNull();
});
