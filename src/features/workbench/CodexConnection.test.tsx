import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {DesktopOverview,type OverviewApi} from './SettingsHome';
import {HostPiAvailability} from './HostPiAvailability';
import {CodexConnection} from './CodexConnection';
afterEach(cleanup);
it('Mac connects only after owner action and retains independent backend capabilities',async()=>{
 const connect=vi.fn(async()=>({connected:true}));
 render(<CodexConnection identity={async()=>({platform:'macos'})} status={async()=>({connected:false})} connect={connect}/>);
 expect(await screen.findByText(/Goal 控制和模型失败后的自动恢复仍可使用/)).toBeVisible();
 expect(connect).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'连接 Codex'}));
 await waitFor(()=>expect(connect).toHaveBeenCalledOnce());
 expect(await screen.findByText('独立后台已连接')).toBeVisible();
 expect(screen.queryByRole('button',{name:/共享|管理员|退役/})).toBeNull();
});
it('an accepted connection request is not reported as a ready backend',async()=>{
 render(<CodexConnection identity={async()=>({platform:'macos'})} status={async()=>({connected:false})} connect={async()=>({connected:false,connecting:true})}/>);
 fireEvent.click(await screen.findByRole('button',{name:'连接 Codex'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'连接中…'})).toBeDisabled());
 expect(screen.queryByText('独立后台已连接')).toBeNull();
});

it('Mac settings describe the independent backend rather than an unsupported shared connection',async()=>{
 const api={shared:async()=>({state:'UNSUPPORTED',enabled:false,ready:false,desktopConnected:false}),pi:vi.fn(),devices:async()=>[],update:async()=>({state:'UNSUPPORTED',currentVersion:'fixture'})} as unknown as OverviewApi;
 render(<HostPiAvailability identity={async()=>({hostName:'fixture',platform:'macos',features:{pi:false}})}><DesktopOverview api={api} piAvailable={false} onNavigate={vi.fn()}/></HostPiAvailability>);
 expect(await screen.findByText('使用独立 app-server')).toBeVisible();
 expect(screen.queryByText('Codex 共享连接')).toBeNull();expect(api.pi).not.toHaveBeenCalled();
});
