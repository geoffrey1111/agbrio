import {createRoot} from 'react-dom/client';
import {NotificationDeliveryPanel,type DeliveryApi,type DeliveryView} from '../../src/features/workbench/NotificationDeliveryPanel';
import '../../src/styles/unified-workbench-v3.css';
import '../../src/styles/warm-workbench.css';
import '../../src/styles/native-router.css';
const params=new URLSearchParams(location.search);
const desktop=params.get('surface')!=='mobile';
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
createRoot(document.getElementById('root')!).render(<main style={{maxWidth:900,margin:'24px auto',padding:16}}><p>离线工程验证 · 无真实钥匙串、账号或邮件发送</p><NotificationDeliveryPanel api={api}/></main>);
