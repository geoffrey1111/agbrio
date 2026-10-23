import {afterEach,expect,it} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {ConnectionGuide,PwaInstallGuide} from './ConnectionGuides';
import {setLanguagePreference} from '../../i18n';
afterEach(()=>{cleanup();setLanguagePreference('zh-CN');});
it('keeps shared preparation distinct from connecting the window and pairing a phone',()=>{
 render(<ConnectionGuide kind="shared" phase={2} open/>);
 const current=document.querySelector('[aria-current="step"]');
 expect(current).toHaveTextContent('从这里打开 Codex');
 expect(screen.getByText(/当前任务结束后，自行退出/)).toBeVisible();
 expect(screen.getByText(/手机网址与配对在/)).toBeVisible();
});
it('explains activation and device pairing without exposing any example credential',()=>{
 render(<ConnectionGuide kind="phone" open/>);
 expect(screen.getByText(/兑换码用于开通入口，配对码用于登录设备/)).toBeVisible();
 expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
});
it('switches the install tutorial by keyboard and keeps instructions matched to the platform',()=>{
 render(<PwaInstallGuide expanded/>);
 expect(screen.getByRole('tabpanel')).toHaveTextContent('使用 Safari');
 fireEvent.keyDown(screen.getByRole('tab',{name:'iPhone / iPad'}),{key:'ArrowRight'});
 expect(screen.getByRole('tab',{name:'Android'})).toHaveFocus();
 expect(screen.getByRole('tab',{name:'Android'})).toHaveAttribute('aria-selected','true');
 expect(screen.getByRole('tabpanel')).toHaveTextContent('使用 Chrome');
 expect(screen.getByRole('tabpanel')).not.toHaveTextContent('Safari');
 expect(screen.getByRole('link',{name:'查看系统官方说明'})).toHaveAttribute('href',expect.stringContaining('support.google.com'));
});
it('localizes practical instructions and uses unique accessible panels for multiple guides',()=>{
 setLanguagePreference('en');render(<><PwaInstallGuide expanded/><PwaInstallGuide expanded/></>);
 expect(screen.getAllByText('Open the Share menu')).toHaveLength(2);
 const panels=screen.getAllByRole('tabpanel');expect(panels[0].id).not.toBe(panels[1].id);
});
