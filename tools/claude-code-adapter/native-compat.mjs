// Quota-free native SDK verification: no session, sign-in, model or real network.
import {mkdtemp,mkdir,readFile,copyFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
const [engine,scratch,mode='all']=process.argv.slice(2);
if(!engine||!scratch)throw new Error('Usage: node native-compat.mjs <Claude executable> <existing task scratch directory>');
if(!['all','wait'].includes(mode))throw new Error('Native test mode must be all or wait');
const plugin=path.resolve(import.meta.dirname,'../claude-code-plugin');
const fixture=await mkdtemp(path.join(path.resolve(scratch),'agbrio-native-'));
await mkdir(path.join(fixture,'.claude-plugin'));await mkdir(path.join(fixture,'hooks'));
const manifest=JSON.parse(await readFile(path.join(plugin,'.claude-plugin/plugin.json'),'utf8'));
manifest.userConfig.hostUrl.default='http://127.0.0.1:12345';manifest.userConfig.token.default='a'.repeat(64);
await writeFile(path.join(fixture,'.claude-plugin/plugin.json'),JSON.stringify(manifest));
for(const file of ['register.mjs','owner-requests.mjs','codex-wait.mjs','codex-notices.mjs','resource-notice.mjs','hooks.json'])await copyFile(path.join(plugin,'hooks',file),path.join(fixture,'hooks',file));
for(const file of mode==='wait'?['native-wait.test.ts','native-task-notices.test.ts']:['native-compat.test.ts','native-wait.test.ts','native-task-notices.test.ts'])await copyFile(path.join(import.meta.dirname,file),path.join(fixture,file));
const result=spawnSync(path.resolve(engine),['plugin','test',fixture],{windowsHide:true,encoding:'utf8'});
process.stdout.write(result.stdout??'');process.stderr.write(result.stderr??'');
process.stdout.write(JSON.stringify({fixture,realProviderCalls:0,mode:'NATIVE_ENGINE_TEST_KIT_ONLY',status:result.status})+'\n');
if(result.error)throw result.error;process.exitCode=result.status??1;
