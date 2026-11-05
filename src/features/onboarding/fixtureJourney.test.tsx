import {act,fireEvent,screen,waitFor} from '@testing-library/react';
import {expect,it} from 'vitest';
/** Drives the isolated preview entry end to end; demo windows stand in for the phone, browser and Claude Code. */
it('the preview entry completes both agents in seven goal screens with one copy and only real external actions',async()=>{
 document.body.innerHTML='<div id="root"></div>';
 await act(async()=>{await import('../../../tests/first-run/fixture');});
 fireEvent.click(await screen.findByLabelText(/Codex共享/));fireEvent.click(screen.getByLabelText(/Claude Code安装/));
 const primary=()=>document.querySelector('.first-run-primary') as HTMLButtonElement;
 const next=async()=>{await act(async()=>{fireEvent.click(primary());});};
 const demo=async(name:RegExp)=>{const button=await screen.findByRole('button',{name},{timeout:5000});await act(async()=>{fireEvent.click(button);});};
 const headings=new Set<string>();const seen=()=>headings.add(document.querySelector('.first-run-shell h1')?.textContent??'');
 seen();await next();seen();await next();
 seen();expect(screen.getByRole('heading',{name:'连接你的手机'})).toBeVisible();await next();
 await demo(/在演示手机登录/);await waitFor(()=>expect(primary()).toHaveTextContent('继续'),{timeout:5000});await next();
 seen();expect(screen.getByRole('heading',{name:'连接 Claude Code'})).toBeVisible();await next();
 await demo(/允许并连接/);await waitFor(()=>expect(primary()).toHaveTextContent('复制验证指令'),{timeout:5000});await next();
 await demo(/粘贴并发送到演示 Code/);expect(screen.getByText(/agbrio_read_app 成功/)).toBeVisible();
 await waitFor(()=>expect(primary()).toHaveTextContent('Code 已返回本机状态'),{timeout:6000});await next();
 seen();await waitFor(()=>expect(screen.getByLabelText(/Claude Code · 我的管理助手/)).toBeChecked());await next();
 seen();await demo(/从演示手机发送/);await next();
 seen();expect(screen.getByText('演示 MCP 读取成功')).toBeVisible();await next();
 await waitFor(()=>expect(screen.getByRole('heading',{name:'完整引导体验结束'})).toBeVisible());
 expect(headings.size).toBe(7);
 const events=(window as unknown as {firstRunDemo:{events:string[]}}).firstRunDemo.events;
 for(const required of ['CODEX_PREPARE','CODEX_OPEN','TRIAL','PHONE_PAIRED','CLAUDE_INSTALL','GRANT','MCP_CONFIGURED','AUTH_PAGE_OPENED','OAUTH_ALLOWED','MCP_READ_VERIFIED','PHONE_REPLY_VERIFIED'])expect(events).toContain(required);
 expect(events.filter(e=>e==='PAIR_CODE')).toHaveLength(1);
},30000);
