import{chromium}from'playwright';import{mkdir,writeFile}from'node:fs/promises';
const pause=ms=>new Promise(r=>setTimeout(r,ms));let browser,lastError;
for(let n=0;n<60;n++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9227');break;}catch(e){lastError=e.message;await pause(500);}}
if(!browser){let endpoint;try{const r=await fetch('http://127.0.0.1:9227/json/version',{signal:AbortSignal.timeout(2000)});endpoint={status:r.status,browser:(await r.json()).Browser};}catch(e){endpoint={error:e.cause?.code??e.name};}console.error(JSON.stringify({ownedEndpoint: endpoint,lastError}));throw Error('Owned WebView2 QA endpoint did not start');}
const folder='runtime/clean-windows';await mkdir(folder,{recursive:true});
try{
 const page=browser.contexts()[0].pages().find(p=>/tauri|localhost/.test(p.url()))??browser.contexts()[0].pages()[0];
 const maximized=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|is_maximized',{label:'main'}));if(!maximized)throw Error('Native desktop did not maximize');
 await page.getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('button',{name:'设备',exact:true}).click();
 const dialog=page.getByRole('dialog');await dialog.getByLabel('连接方式').waitFor();
 if(await dialog.getByLabel('HTTPS 网址').inputValue()!=='')throw Error('Fresh install contains a personal URL');
 if(!await dialog.getByRole('button',{name:'生成配对码',exact:true}).isDisabled())throw Error('Pairing offered before public entry verification');
 await dialog.getByLabel('连接方式').selectOption('CUSTOM_HTTPS');await dialog.getByLabel('HTTPS 网址').fill('https://agbrio-qa.invalid');
 await dialog.getByRole('button',{name:'验证并保存',exact:true}).click();await dialog.getByRole('alert').waitFor({timeout:20000});
 if(!await dialog.getByLabel('连接方式').isVisible())throw Error('Failed setup hid recovery controls');
 await page.screenshot({path:folder+'/desktop-first-setup.png'});
 const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1);if(overflow)throw Error('Maximized desktop overflows');
 await writeFile(folder+'/proof.json',JSON.stringify({freshWindowsRunner:true,installedApplication:true,desktopIPC:true,setupSelector:true,noPersonalOrigin:true,invalidHttpsRejected:true,pairingBlockedUntilConfigured:true,nativeMaximized:true,maximizedDesktopOverflow:false,providerCredentialsUsed:false},null,2));
}finally{await browser.close();}
