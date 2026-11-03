import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ClaudeNativeConnection} from './ClaudeNativeConnection';
import macCommand from './claudeManualInstall.macos.sh?raw';
afterEach(cleanup);
it('offers the Mac fallback after native failure and copies it without invoking installation again',async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);
 Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText}});
 const connect=vi.fn().mockRejectedValue('CLAUDE_PLUGIN_ADMIN_REFUSED:INSTALL:1');
 render(<ClaudeNativeConnection connect={connect}/>);
 fireEvent.click(screen.getByRole('button',{name:/安装 Claude Code 插件|启用 Claude 通知与手机回复/}));
 await screen.findByRole('alert');
 fireEvent.click(screen.getByRole('button',{name:'复制 macOS 手动安装命令'}));
 await waitFor(()=>expect(writeText).toHaveBeenCalledWith(macCommand));
 expect(connect).toHaveBeenCalledTimes(1);
 expect(screen.getByRole('button',{name:'复制手动安装命令'})).toBeVisible();
});
