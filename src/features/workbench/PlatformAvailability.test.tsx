import {afterEach, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen} from '@testing-library/react';
import {SharedCodexConnection} from './SharedCodexConnection';
import {PiSettings} from './PiSettings';
import {DesktopUpdateSettings, type UpdateApi} from './DesktopUpdateSettings';
import {ResourceMonitorSettings} from './ResourceMonitorSettings';

afterEach(cleanup);

it('unsupported shared Codex exposes neither Windows administrator controls nor a launch action', async () => {
  const setup = vi.fn();
  render(<SharedCodexConnection api={{status:async()=>({enabled:false,ready:false,desktopConnected:false,state:'UNSUPPORTED'}),setup}}/>);
  await screen.findByText('该平台尚未支持');
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect(screen.queryByRole('button')).toBeNull();
  expect(setup).not.toHaveBeenCalled();
});

it('unsupported Pi is distinct from a missing installation and cannot grant a workspace', async () => {
  const authorize = vi.fn();
  render(<PiSettings api={{read:async()=>({platformSupported:false,installed:false,version:null,scopes:[]}),authorize,revoke:vi.fn()}}/>);
  await screen.findByText('该平台尚未支持');
  expect(screen.queryByRole('textbox')).toBeNull();
  expect(authorize).not.toHaveBeenCalled();
});

it('unsupported updates do not perform a check, download or install', async () => {
  const api:UpdateApi={status:async()=>({state:'UNSUPPORTED',currentVersion:'0.1.0',version:null,notes:null,releaseUrl:null,downloaded:0,total:null,error:'PLATFORM_NOT_YET_SUPPORTED'}),check:vi.fn(),download:vi.fn(),install:vi.fn(),openRelease:vi.fn()};
  render(<DesktopUpdateSettings updateApi={api}/>);
  await screen.findByText('该平台尚未支持');
  expect(screen.queryByRole('button')).toBeNull();
  expect(api.check).not.toHaveBeenCalled();
  expect(api.download).not.toHaveBeenCalled();
  expect(api.install).not.toHaveBeenCalled();
});

it('resource monitoring discloses platform unavailability rather than a transient disk read failure', async () => {
  const apply=vi.fn();
  render(<ResourceMonitorSettings api={{read:async()=>{throw Error('PLATFORM_NOT_YET_SUPPORTED');},apply}}/>);
  const details=screen.getByText('监测磁盘与预警线').closest('details')!;
  details.open=true;fireEvent(details,new Event('toggle'));
  await screen.findByText('该平台尚未支持');
  expect(apply).not.toHaveBeenCalled();
});
