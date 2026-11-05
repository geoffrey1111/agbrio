// Maintained project integration test, never an agent browser or real tenant.
import {Miniflare} from 'miniflare';
import {chromium} from 'playwright';
import fs from 'node:fs/promises';import path from 'node:path';import os from 'node:os';
import {spawn,execFileSync} from 'node:child_process';import https from 'node:https';import {randomBytes,createHash,X509Certificate} from 'node:crypto';import {fileURLToPath,pathToFileURL} from 'node:url';import assert from 'node:assert/strict';
const repo=path.resolve(fileURLToPath(new URL('../..',import.meta.url)));
let selected=process.argv[2];
if(selected==='--cargo-messages'){
 const messages=(await fs.readFile(process.argv[3],'utf8')).split('\n').flatMap(line=>{try{return [JSON.parse(line)];}catch{return [];}});
 const binaries=messages.filter(m=>m.reason==='compiler-artifact'&&m.target?.name==='ai_work_router_lib'&&m.profile?.test&&m.executable).map(m=>m.executable);
 assert.equal(binaries.length,1,'exact one Rust lib test executable');selected=binaries[0];
}
const binary=path.resolve(selected??'');assert((await fs.stat(binary)).isFile(),'exact Rust test binary required');
const root=await fs.mkdtemp(path.join(process.env.RUNNER_TEMP??(process.platform==='win32'?'D:/desktop/临时处理':os.tmpdir()),'agbrio-hosted-e2e-'));let service,browser,child;let logs='';
const admin=randomBytes(32).toString('hex');const tenant='a'.repeat(32),trialTenant='b'.repeat(32);
const control='https://control.agbrio.localhost';
const openssl=process.env.AGBRIO_FIXTURE_OPENSSL??(process.platform==='win32'?'openssl.exe':'/usr/bin/openssl');
try{
 await fs.writeFile(path.join(root,'fixture-only'),'isolated service and profile only');
 await fs.writeFile(path.join(root,'tls.cnf'),'[req]\ndistinguished_name=dn\nprompt=no\nx509_extensions=fixture\n[dn]\nCN=Agbrio isolated fixture\n[fixture]\nsubjectAltName=DNS:*.agbrio.localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n');
 execFileSync(openssl,['req','-x509','-newkey','rsa:2048','-nodes','-sha256','-days','1','-keyout',path.join(root,'key.pem'),'-out',path.join(root,'cert.pem'),'-config',path.join(root,'tls.cnf')],{stdio:'ignore'});
 const cert=await fs.readFile(path.join(root,'cert.pem'));const key=await fs.readFile(path.join(root,'key.pem'));
 service=new Miniflare({modules:true,scriptPath:path.join(repo,'services/hosted-relay/worker.mjs'),compatibilityDate:'2026-08-06',cf:false,host:'127.0.0.1',port:443,httpsCert:cert.toString(),httpsKey:key.toString(),bindings:{CONTROL_HOST:'control.agbrio.localhost',ZONE_ROOT:'agbrio.localhost',ADMIN_TOKEN:admin},durableObjects:{CONTROL:{className:'Control',useSQLite:true},TENANTS:{className:'Tenant',useSQLite:true}},durableObjectsPersist:false});
 await service.ready;
 const request=(url,body,adminRequest=false)=>new Promise((resolve,reject)=>{const u=new URL(url);const req=https.request(u,{method:body?'POST':'GET',ca:cert,lookup:(_host,options,cb)=>options.all?cb(null,[{address:'127.0.0.1',family:4}]):cb(null,'127.0.0.1',4),headers:{...(body?{'content-type':'application/json'}:{}),...(adminRequest?{authorization:'Bearer '+admin}:{})},timeout:10000},res=>{const chunks=[];res.on('data',c=>chunks.push(c));res.on('end',()=>resolve({status:res.statusCode,text:Buffer.concat(chunks).toString()}));});req.on('error',reject);req.on('timeout',()=>req.destroy(Error('fixture request timeout')));req.end(body?JSON.stringify(body):undefined);});
 const codes=[7,30].map(days=>({days,code:'AGB'+days+'-'+randomBytes(32).toString('hex'),tenantId:tenant}));
 // Different code batches may renew one existing tenant without changing server policy.
 for(const c of codes){c.hash=createHash('sha256').update(c.code).digest('hex');const result=await request(control+'/admin/issue',{codes:[{days:c.days,hash:c.hash,tenantId:c===codes[0]?tenant:'c'.repeat(32)}]},true);assert.equal(result.status,200,result.text);}
 assert.equal((await request(control+'/admin/ready',{hashes:codes.map(c=>c.hash)},true)).status,200);
 assert.equal((await request(control+'/admin/trials/prepare',{tenantIds:[trialTenant]},true)).status,200);
 assert.equal((await request(control+'/admin/trials/ready',{tenantIds:[trialTenant]},true)).status,200);
 const spki=createHash('sha256').update(new X509Certificate(cert).publicKey.export({type:'spki',format:'der'})).digest('base64');
 browser=await chromium.launch({headless:true,args:['--ignore-certificate-errors-spki-list='+spki]});
 const waitFile=async(name,timeout=90000)=>{const end=Date.now()+timeout;while(Date.now()<end){try{return JSON.parse(await fs.readFile(name,'utf8'));}catch{}if(child?.exitCode!==null&&child?.exitCode!==undefined)throw Error('native fixture exited '+child.exitCode+' '+logs.slice(-3000));await new Promise(r=>setTimeout(r,100));}throw Error('fixture deadline '+path.basename(name)+' '+logs.slice(-3000));};
 const receipts=[];
 for(const trial of [false,true]){
  const profile=path.join(root,trial?'trial-profile':'code-profile');await fs.mkdir(profile);await fs.mkdir(path.join(profile,'home'));await fs.mkdir(path.join(profile,'home/Library/Application Support'),{recursive:true});await fs.mkdir(path.join(profile,'local'));await fs.writeFile(path.join(profile,'fixture-only'),'isolated');await fs.copyFile(path.join(root,'cert.pem'),path.join(profile,'cert.pem'));
  await fs.writeFile(path.join(profile,'input.json'),JSON.stringify(trial?{trial:true}:{code:codes[0].code}));
  const env={...process.env,HOME:path.join(profile,'home'),USERPROFILE:path.join(profile,'home'),LOCALAPPDATA:path.join(profile,'local'),APPDATA:path.join(profile,'local'),CODEX_HOME:path.join(profile,'home/.codex'),AGBRIO_HOSTED_FIXTURE_ROOT:profile,AGBRIO_FIXTURE_NODE:process.env.AGBRIO_FIXTURE_NODE??process.execPath,NODE_EXTRA_CA_CERTS:path.join(root,'cert.pem'),NODE_OPTIONS:'--import='+pathToFileURL(path.join(repo,'tools/hosted-relay-fixture/fixture-dns.mjs')).href};
  for(const key of Object.keys(env))if(/^(OPENAI|ANTHROPIC|TAURI_SIGNING|CLOUDFLARE|CF_API|GITHUB_TOKEN|GH_TOKEN)/.test(key))delete env[key];
  logs='';child=spawn(binary,['hosted_relay::fixture::native_hosted_transport','--exact','--ignored','--nocapture','--test-threads=1'],{cwd:repo,env,stdio:['ignore','pipe','pipe']});child.stdout.on('data',b=>{logs=(logs+b).slice(-12000)});child.stderr.on('data',b=>{logs=(logs+b).slice(-12000)});
  const ready=await waitFile(path.join(profile,'ready.json'));assert.equal(ready.kind,trial?'TRIAL':'CODE');
  const context=await browser.newContext({locale:'zh-CN',viewport:{width:390,height:844}});const page=await context.newPage();
  await page.goto(ready.origin+'/mobile');await page.getByLabel('6 位配对码').fill(ready.pairCode);await page.getByRole('button',{name:'连接工作台',exact:true}).click();
  await page.getByText('Hosted transport fixture',{exact:true}).click();
  await page.getByText('Fixture ready',{exact:true}).first().waitFor({state:'visible'});
  // API uses the same authenticated browser origin and real Host command route.
  const replyId=crypto.randomUUID();const result=await page.evaluate(async({id})=>{
   const read=await (await fetch('/v1/mobile/codex-watches/chat/fixture-chat')).json();
   const w=read.watch; if(!w)throw Error('fixture watch DTO missing: '+JSON.stringify(read));
   const input={action:'SEND',threadId:w.threadId,id,generation:w.generation,expectedTurnId:w.snapshot?.turnId??'fixture-seed',mode:'SEND',text:'Synthetic phone follow-up',options:{}};
   const post=()=>fetch('/v1/mobile/codex-watches/chat',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(input)}).then(async r=>({status:r.status,body:await r.json()}));
   return {first:await post(),replay:await post()};
  },{id:replyId});
  assert.equal(result.first.status,200,JSON.stringify(result));assert.equal(result.replay.status,200,JSON.stringify(result));
  const sends=(await fs.readFile(path.join(profile,'native-sends.jsonl'),'utf8')).trim().split('\n');assert.equal(sends.length,1,'exact native request is never replayed');
  await page.reload();await page.getByText('Hosted transport fixture',{exact:true}).first().click();await page.getByText('Fixture reply received',{exact:true}).first().waitFor({state:'visible'});
  if(!trial){await fs.writeFile(path.join(profile,'renew.json'),JSON.stringify({code:codes[1].code}));const renewed=await waitFile(path.join(profile,'renewed.json'));assert.equal(renewed.origin,ready.origin);assert(renewed.expiresAt>ready.expiresAt);}
  await context.close();await fs.writeFile(path.join(profile,'stop'),'done');const exit=await new Promise(resolve=>{if(child.exitCode!==null)resolve(child.exitCode);else child.once('exit',resolve);});assert.equal(exit,0,logs);assert(logs.includes('HOSTED_TRANSPORT_NATIVE_PASS'));child=null;
  receipts.push({operation:trial?'TRIAL':'CODE_AND_RENEWAL',realTls:true,realWss:true,unchangedWorkerSource:true,fixtureCompatibilityDate:'2026-08-06',deployedCompatibilityDate:'2026-10-01',productionConnector:true,productionRustActivation:true,pairedBrowser:true,messageSendAndRead:true,replaySuppressed:true,syntheticNativeProvider:true,macRunner:process.platform==='darwin',physicalDevice:false});
 }
 console.log(JSON.stringify({status:'HOSTED_E2E_PASS',receipts}));if(process.platform==='darwin')console.log('MACOS_HOSTED_E2E_PASS');
}catch(error){console.error('Native fixture failure log:',logs);for(const name of ['code-profile','trial-profile']){console.error('Protocol trace',await fs.readFile(path.join(root,name,'native-protocol-errors.jsonl'),'utf8').catch(()=>''));}throw error;}finally{if(child&&child.exitCode===null){child.kill();await new Promise(resolve=>child.once('exit',resolve));}await browser?.close();await service?.dispose();assert(path.basename(root).startsWith('agbrio-hosted-e2e-')&&!(await fs.lstat(root)).isSymbolicLink());await fs.rm(root,{recursive:true,force:true});}
