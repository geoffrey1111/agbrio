// Disposable CI listener only, never bundled or installed as an app/helper.
import {Miniflare} from 'miniflare';import fs from 'node:fs/promises';import path from 'node:path';import {fileURLToPath} from 'node:url';
export async function createService(root){
 const input=JSON.parse(await fs.readFile(path.join(root,'service-input.json'),'utf8'));
 const repo=path.resolve(fileURLToPath(new URL('../..',import.meta.url)));
 return new Miniflare({modules:true,scriptPath:path.join(repo,'services/hosted-relay/worker.mjs'),compatibilityDate:'2026-08-06',cf:false,host:'127.0.0.1',port:443,httpsCert:await fs.readFile(path.join(root,'cert.pem'),'utf8'),httpsKey:await fs.readFile(path.join(root,'key.pem'),'utf8'),bindings:{CONTROL_HOST:'control.agbrio.localhost',ZONE_ROOT:'agbrio.localhost',ADMIN_TOKEN:input.admin},durableObjects:{CONTROL:{className:'Control',useSQLite:true},TENANTS:{className:'Tenant',useSQLite:true}},durableObjectsPersist:false});
}
if(process.argv[2]==='--ci-listener'){
 if(process.platform!=='darwin'||process.env.GITHUB_ACTIONS!=='true'||process.getuid()!==0)throw Error('EXPLICIT_MAC_CI_LISTENER_ONLY');
 const root=await fs.realpath(process.argv[3]);if(!path.basename(root).startsWith('agbrio-hosted-e2e-')||(await fs.lstat(root)).isSymbolicLink())throw Error('FIXTURE_ROOT_INVALID');
 const temporary=path.join(root,'service-tmp');let service;
 try{service=await createService(root);await service.ready;await fs.writeFile(path.join(root,'service-ready.json'),'{"ready":true}');process.stdin.resume();await new Promise(resolve=>{const timeout=setTimeout(resolve,300000);process.stdin.once('end',()=>{clearTimeout(timeout);resolve();});});}
 finally{await service?.dispose();const resolved=await fs.realpath(temporary);if(resolved!==path.join(root,'service-tmp'))throw Error('FIXTURE_TEMP_CHANGED');await fs.rm(resolved,{recursive:true,force:true});}
}
