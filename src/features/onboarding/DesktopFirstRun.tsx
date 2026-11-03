import {useEffect,useState,type ReactNode} from 'react';
import {isTauri} from '@tauri-apps/api/core';
import {ListChecks} from 'lucide-react';
import {t} from '../../i18n';
import {FirstRunWizard,type FirstRunPreview} from './FirstRunWizard';
import {desktopFirstRunApi,type FirstRunApi} from './firstRunApi';
import {OPEN_SETUP_EVENT,readSetupPreference,writeSetupPreference,setupStorage,type SetupStep,type SetupPreference,type SetupSnapshot} from './firstRunModel';

/** UI progress only; never a source of connection, permission or OAuth authority. */
export function DesktopFirstRun({children,api=desktopFirstRunApi,native=isTauri(),storage=setupStorage,companion,preview}:{children:ReactNode;preview?:FirstRunPreview;companion?:(step:SetupStep)=>ReactNode;api?:FirstRunApi;native?:boolean;storage?:Pick<Storage,'getItem'|'setItem'>}){
 const [checking,setChecking]=useState(native),[launched,setLaunched]=useState(!native),[open,setOpen]=useState(false),[initial,setInitial]=useState<SetupSnapshot>({}),[resume,setResume]=useState<SetupPreference|null>(null);
 useEffect(()=>{if(!native){setChecking(false);return;}let alive=true;let startupTimer:ReturnType<typeof setTimeout>|undefined;const saved=readSetupPreference(storage);
  if(saved&&saved.disposition!=='PENDING'){setChecking(false);setLaunched(true);}else{startupTimer=setTimeout(()=>{if(alive){setResume(saved);setOpen(true);setChecking(false);alive=false;}},3000);void (api.eligibility&&!saved?api.eligibility():api.snapshot(saved?.agents??[])).then(snapshot=>{if(!alive)return;clearTimeout(startupTimer);setInitial(snapshot);if(!saved&&snapshot.existingUse===true){writeSetupPreference(storage,{version:1,disposition:'EXISTING',agents:[],step:'CHOOSE'});setLaunched(true);}else{setResume(saved);setOpen(true);}setChecking(false);}).catch(()=>{if(alive){clearTimeout(startupTimer);setResume(saved);setOpen(true);setChecking(false);}});}
  const reopen=()=>{setResume({...readSetupPreference(storage)??{version:1,disposition:'PENDING',agents:[]},version:1,disposition:'PENDING',step:'CHOOSE'});setOpen(true);};window.addEventListener(OPEN_SETUP_EVENT,reopen);return()=>{alive=false;clearTimeout(startupTimer);window.removeEventListener(OPEN_SETUP_EVENT,reopen);};
 },[api,native,storage]);
 return <>{!checking&&launched&&children}{checking&&<div className="first-run-loading" role="status">{t('正在检查首次设置…')}</div>}{native&&open&&<FirstRunWizard preview={preview} companion={companion} api={api} storage={storage} initial={initial} preference={resume} onExit={()=>{setOpen(false);setLaunched(true);}}/>}</>;
}
export function FirstRunLauncher(){if(!isTauri())return null;return <button className="first-run-launcher" type="button" onClick={()=>window.dispatchEvent(new Event(OPEN_SETUP_EVENT))}><ListChecks size={17} aria-hidden/>{t('重新运行设置引导')}</button>;}
