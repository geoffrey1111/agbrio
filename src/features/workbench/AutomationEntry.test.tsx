import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {AutomationEntry} from './AutomationEntry';

afterEach(()=>{cleanup();vi.unstubAllGlobals();});

it('opens a single launcher and copies the real endpoint and installed CLI command',async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);
 vi.stubGlobal('navigator',Object.assign(Object.create(navigator),{clipboard:{writeText}}));
 const api={mcp:vi.fn().mockResolvedValue('https://example.test/mcp'),cli:vi.fn().mockResolvedValue("D:/Owner's Programs/Agbrio/app-control/control.ps1")};
 render(<AutomationEntry api={api}/>);
 expect(api.mcp).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'MCP & CLI'}));
 expect(await screen.findByRole('dialog',{name:'Agbrio MCP & CLI'})).toBeVisible();
 fireEvent.click(await screen.findByRole('button',{name:'复制提示词'}));
 fireEvent.click(await screen.findByRole('button',{name:'复制命令'}));
 await waitFor(()=>expect(writeText).toHaveBeenCalledTimes(2));
 expect(writeText.mock.calls[0][0]).toContain('https://example.test/mcp');
 expect(writeText.mock.calls[0][0]).toContain('agbrio_capabilities');
 expect(writeText.mock.calls[1][0]).toBe("pwsh -NoProfile -File 'D:/Owner''s Programs/Agbrio/app-control/control.ps1' -Command Capabilities");
 fireEvent.click(screen.getByRole('button',{name:'关闭'}));
 expect(screen.queryByRole('dialog')).toBeNull();
});

it('routes missing MCP setup to the actual settings and does not offer a fabricated CLI path',async()=>{
 const onConfigure=vi.fn();
 render(<AutomationEntry api={{mcp:async()=>null,cli:async()=>null}} onConfigure={onConfigure}/>);
 fireEvent.click(screen.getByRole('button',{name:'MCP & CLI'}));
 expect(await screen.findByText('暂时无法读取本机 CLI 路径。')).toBeVisible();
 expect(screen.queryByRole('button',{name:'复制提示词'})).toBeNull();
 expect(screen.queryByRole('button',{name:'复制命令'})).toBeNull();
 fireEvent.click(await screen.findByRole('button',{name:'打开 AI 助手设置'}));
 expect(onConfigure).toHaveBeenCalledOnce();
 expect(screen.queryByRole('dialog')).toBeNull();
});
