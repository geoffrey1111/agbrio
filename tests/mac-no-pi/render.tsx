// Maintained offline rendering fixture. No real Host, provider or account.
import {createRoot} from 'react-dom/client';
import {useState} from 'react';
import {mockIPC} from '@tauri-apps/api/mocks';
import {UnifiedWorkbench} from '../../src/features/workbench/UnifiedWorkbench';
import type {OverviewApi} from '../../src/features/workbench/SettingsHome';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/warm-workbench.css';
import '../../src/styles/web-login.css';
import '../../src/styles/watch-chat.css';
import '../../src/styles/bridge-v5.css';
import '../../src/styles/native-router.css';
import '../../src/styles/interaction-motion.css';
import '../../src/styles/conversation-v6.css';
import '../../src/styles/agbrio-v6-theme.css';
import '../../src/styles/conversation-polish.css';
import '../../src/styles/agbrio-v7.css';
import '../../src/styles/agbrio-v7-r2.css';
const windows=new URLSearchParams(location.search).get('host')==='windows';
mockIPC(command=>command==='plugin:event|listen'?1:command==='attachment_cache_status'?{groups:[],totalBytes:0}:{});
const identity=async()=>({hostName:windows?'Fixture Windows':'Fixture Mac',platform:windows?'windows':'macos',features:{pi:windows}});
const overview:OverviewApi={shared:async()=>({enabled:false,ready:false,desktopConnected:false,state:'UNSUPPORTED',updatePending:false,launchAsAdministrator:false,currentAdministrator:null,launchProgress:null}),pi:async()=>({installed:false}),devices:async()=>[],update:async()=>({state:'UNSUPPORTED',currentVersion:'fixture'}) as Awaited<ReturnType<OverviewApi['update']>>};
const watchApi={watches:async()=>[],connect:async()=>{},threads:async()=>({threads:[]})} as import('../../src/features/workbench/CodexNotifications').NotificationApi;
function Fixture(){
 const [surface,setSurface]=useState<import('../../src/features/workbench/UnifiedWorkbench').WorkbenchSurface>('SETTINGS');
 return <><div style={{position:'fixed',bottom:4,left:4,zIndex:10000,fontSize:11,pointerEvents:'none',background:'#fff'}}>隔离测试 · 模拟{windows?'Windows':'Mac'}主机 · 无真实连接</div><UnifiedWorkbench items={[]} draft={{value:''}} surface={surface} onSurfaceChange={setSurface} onDraftChange={()=>{}} onSelectWorkstream={()=>{}} computerIdentity={identity} overviewApi={overview} piSettingsPanel={<p>Pi 测试面板</p>} codexConnectionPanel={<p>共享连接测试面板</p>} assistantPanel={<p>助手测试面板</p>} watchApi={watchApi}/></>;
}
createRoot(document.getElementById('root')!).render(<Fixture/>);
