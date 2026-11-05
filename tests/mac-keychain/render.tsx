import {mockIPC} from '@tauri-apps/api/mocks';
import {AssistantSettings,type AssistantSettingsApi} from '../../src/features/workbench/AssistantSettings';
import type {EventKeychainApi} from '../../src/features/workbench/EventKeychainSettings';
import {createRoot} from 'react-dom/client';
import {NotificationDeliveryPanel,type DeliveryApi,type DeliveryView} from '../../src/features/workbench/NotificationDeliveryPanel';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/warm-workbench.css';
import '../../src/styles/native-router.css';
const params=new URLSearchParams(location.search);
const desktop=params.get('surface')!=='mobile';
mockIPC(command=>command==='plugin:event|listen'?1:{});
const eventCalls:(string|null)[]=[];
const eventApi:EventKeychainApi={status:async()=>({supported:params.get('platform')!=='windows',storedReferences:65,pendingCleanup:1,needsOwnerAction:true}),authorize:async after=>{
 eventCalls.push(after);document.documentElement.dataset.eventCalls=JSON.stringify(eventCalls);
 if(params.has('denied'))throw new Error('KEYCHAIN_OWNER_ACTION_REQUIRED');
 return {checked:after?1:64,nextCursor:after?null:'agbrio-native-key:v1:89a21534-a51e-49e8-8a9b-41b06262f344',notificationSent:false};
}};
const assistantApi:AssistantSettingsApi={view:async()=>({bridges:[],grants:[],mcpUrl:'https://fixture.invalid/mcp'}),create:async()=>({}),revoke:async()=>({}),...(desktop?{eventsKeychain:eventApi}:{})};
let view:DeliveryView={desktopPlatform:'MACOS',credentialStatus:'NEEDS_OWNER_ACTION',account:'fixture@example.invalid',recipient:'fixture@example.invalid',hasSecret:false,windowsStatus:'ENABLED',channels:[{channel:'EMAIL',enabled:false}],history:[]};
const calls:string[]=[];
const api:DeliveryApi={desktop,settings:async()=>structuredClone(view),command:async raw=>{
 const input=raw as {action:string};calls.push(input.action);document.documentElement.dataset.calls=JSON.stringify(calls);
 if(input.action==='AUTHORIZE_EMAIL_KEYCHAIN'){
  if(params.has('denied'))throw new Error('KEYCHAIN_OWNER_ACTION_REQUIRED');
  view={...view,hasSecret:true,credentialStatus:'READY'};
 }
 return 'SAVED';
}};
createRoot(document.getElementById('root')!).render(<main style={{maxWidth:900,margin:'24px auto',padding:16}}><p>离线工程验证 · 无真实钥匙串、账号或邮件发送</p>{params.has('events')?<AssistantSettings api={assistantApi}/>:<NotificationDeliveryPanel api={api}/>}</main>);
