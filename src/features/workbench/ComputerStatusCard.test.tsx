import {cleanup,render,screen} from '@testing-library/react';
import {afterEach,expect,it} from 'vitest';
import {ComputerStatusCard} from './SettingsHome';
import type {RuntimePresentation,RuntimeCheck} from './models';
import {setLanguagePreference} from '../../i18n';
afterEach(()=>{cleanup();setLanguagePreference('zh-CN');});
const runtime=(checks:[string,RuntimeCheck['state']][]):RuntimePresentation=>({checks:checks.map(([id,state])=>({id,state,label:id,detail:''}))});
it('hides a healthy phone router-core even when legacy checks are absent',()=>{
 const {container}=render(<ComputerStatusCard runtime={runtime([['router-core','READY'],['providers','WARNING']])}/>);
 expect(container).toBeEmptyDOMElement();
});
it('shows a confirmed router-core disconnect even if Codex was ready',()=>{
 render(<ComputerStatusCard runtime={runtime([['router-core','UNAVAILABLE'],['codex','READY']])}/>);
 expect(screen.getByText('暂时连不上')).toBeVisible();
});
it('shows a confirmed Codex disconnect separately from computer connectivity',()=>{
 render(<ComputerStatusCard runtime={runtime([['router-core','READY'],['codex','UNAVAILABLE']])}/>);
 expect(screen.getByText('Codex 连接 · 未就绪')).toBeVisible();
 expect(screen.queryByText('暂时连不上')).toBeNull();
});
it('does not turn missing, checking or warning-only states into an offline alert',()=>{
 const {container,rerender}=render(<ComputerStatusCard/>);
 expect(container).toBeEmptyDOMElement();
 for(const checks of [[],[['router-core','CHECKING']],[['providers','WARNING']],[['router-core','WARNING']]] as [string,RuntimeCheck['state']][][]){
  rerender(<ComputerStatusCard runtime={runtime(checks)}/>);expect(container).toBeEmptyDOMElement();
 }
});
