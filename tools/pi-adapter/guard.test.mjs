import {test} from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import path from 'node:path';import os from 'node:os';import {scopedPath,safeSearch} from './guard.mjs';
import guard,{accessPath,checkedRoot} from './guard.mjs';import crypto from 'node:crypto';import {spawnSync} from 'node:child_process';import {pathToFileURL} from 'node:url';

test('multiple task roots, readonly rule sources and private application state stay distinct',()=>{
 const base=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-roots-'));
 try{const a=path.join(base,'a'),b=path.join(base,'b'),skills=path.join(base,'.codex','skills');for(const p of[a,b,skills])fs.mkdirSync(p,{recursive:true});const skill=path.join(skills,'SKILL.md');fs.writeFileSync(skill,'shared rules');const grants=[a,b].map(p=>({path:p,pinned:checkedRoot(p),writable:true}));const resources={skillRoots:[skills],protectedRoots:[path.join(b,'private')]};
  assert.equal(accessPath(a,path.join(b,'output.txt'),true,grants,resources),path.join(b,'output.txt'));
  assert.equal(accessPath(a,skill,false,grants,resources),skill);assert.throws(()=>accessPath(a,skill,true,grants,resources));
  assert.throws(()=>accessPath(a,path.join(b,'private','state.db'),false,grants,resources));assert.throws(()=>accessPath(a,'C:relative.txt',false,grants,resources));
 }finally{fs.rmSync(base,{recursive:true,force:true});}
});

test('invalid context exits its owned worker even when the SDK catches hook exceptions',()=>{
 const base=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-context-'));
 try{const file=path.join(base,'context.md'),sentinel=path.join(base,'provider-would-run');fs.writeFileSync(file,'expected owner rules');
  const script=`import guard from ${JSON.stringify(pathToFileURL(path.resolve('tools/pi-adapter/guard.mjs')).href)};import fs from 'node:fs';const handlers={};guard({registerCommand(){},on(k,v){handlers[k]=v;},appendEntry(){}});try{await handlers.before_agent_start({systemPromptOptions:{appendSystemPrompt:'wrong',contextFiles:[],skills:[]}});}catch{}fs.writeFileSync(${JSON.stringify(sentinel)},'bad');`;
  const result=spawnSync(process.execPath,['--input-type=module','-e',script],{env:{...process.env,AGBRIO_PI_SCOPE:base,AGBRIO_PI_CAP:'qa-cap',AGBRIO_PI_ACCESS:JSON.stringify({roots:[{path:base,writable:false}],resources:{appendFile:file,appendDigest:crypto.createHash('sha256').update('expected owner rules').digest('hex')}})},encoding:'utf8',windowsHide:true});
  assert.equal(result.status,78);assert.equal(fs.existsSync(sentinel),false);assert.match(result.stdout,/PI_CONTEXT_NOT_LOADED/);
 }finally{fs.rmSync(base,{recursive:true,force:true});}
});

test('native context may shadow a main-worktree rule while the required local rule stays loaded',async()=>{
 const base=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-shadow-'));const previous={...process.env};
 try{const main=path.join(base,'main.md'),local=path.join(base,'local.md');fs.writeFileSync(main,'main rules');fs.writeFileSync(local,'local rules');const hash=text=>crypto.createHash('sha256').update(text).digest('hex');
  process.env.AGBRIO_PI_SCOPE=base;process.env.AGBRIO_PI_CAP='shadow-cap';process.env.AGBRIO_PI_ACCESS=JSON.stringify({roots:[{path:base,writable:false}],resources:{instructions:[{path:main,sha256:hash('main rules'),via:'native',required:false},{path:local,sha256:hash('local rules'),via:'native',required:true}],skillFiles:[]}});
  const commands={},events={},entries=[];guard({registerCommand:(k,v)=>commands[k]=v,on:(k,v)=>events[k]=v,appendEntry:(type,data)=>entries.push({type,data})});
  await commands['agbrio-control'].handler(['shadow-cap','request',hash('payload'),hash('probe'),hash(Buffer.alloc(4))].join(' '));await events.input({source:'rpc',text:'probe',images:[]});
  await events.before_agent_start({systemPromptOptions:{contextFiles:[{path:local,content:'local rules'}],skills:[]}});
  const loaded=entries.find(e=>e.type==='agbrio.resources');assert.deepEqual(loaded.data.instructions.map(f=>f.path),[local]);assert.equal(process.env.AGBRIO_PI_CAP,undefined);
 }finally{for(const k of Object.keys(process.env))if(k.startsWith('AGBRIO_PI_'))delete process.env[k];for(const[k,v]of Object.entries(previous))if(k.startsWith('AGBRIO_PI_'))process.env[k]=v;fs.rmSync(base,{recursive:true,force:true});}
});

test('input acceptance and actual user consumption have separate exact receipts',async()=>{
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-intent-'));const previous={root:process.env.AGBRIO_PI_SCOPE,cap:process.env.AGBRIO_PI_CAP};const commands={},events={},entries=[];
 try{process.env.AGBRIO_PI_SCOPE=root;process.env.AGBRIO_PI_CAP='owned-capability';guard({registerCommand:(k,v)=>commands[k]=v,on:(k,v)=>events[k]=v,appendEntry:(type,data)=>entries.push({type,data})});
  const text='hello\u2028world',hash=crypto.createHash('sha256').update(text).digest('hex'),empty=crypto.createHash('sha256').update(Buffer.alloc(4)).digest('hex'),payload='a'.repeat(64);
  assert.deepEqual(await events.input({source:'rpc',text,images:[]}),{action:'handled'});
  await commands['agbrio-control'].handler(['owned-capability','request1',payload,hash,empty].join(' '));
  assert.deepEqual(await events.input({source:'rpc',text,images:[]}),{action:'continue'});assert.equal(entries.at(-1).type,'agbrio.input');
  await events.message_start({message:{role:'assistant',content:[{type:'text',text}]}});assert.equal(entries.at(-1).type,'agbrio.input');
  await events.message_start({message:{role:'user',content:[{type:'text',text}]}});assert.equal(entries.at(-1).type,'agbrio.consumed');assert.equal(entries.at(-1).data.requestId,'request1');
  await commands['agbrio-control'].handler(['owned-capability','request2',payload,hash,empty].join(' '));await events.input({source:'rpc',text,images:[]});
  await assert.rejects(commands['agbrio-control'].handler(['owned-capability','duplicate',payload,hash,empty].join(' ')),/IDENTICAL_INPUT/);
  await commands['agbrio-forget-control'].handler(['owned-capability','request2',payload].join(' '));
  await commands['agbrio-control'].handler(['owned-capability','replacement',payload,hash,empty].join(' '));await events.input({source:'rpc',text,images:[]});
  await commands['agbrio-clear-control'].handler('owned-capability');const length=entries.length;await events.message_start({message:{role:'user',content:text}});assert.equal(entries.length,length);
  await events.agent_start();await events.agent_start();await events.agent_settled({aborted:false},{sessionManager:{getLeafId:()=> 'native-leaf'}});
  assert.equal(entries.at(-1).type,'agbrio.settled');assert.equal(entries.at(-1).data.leafId,'native-leaf');assert.deepEqual(entries.at(-1).data.requests,[{requestId:'request1',payloadHash:payload}]);
  assert.equal(JSON.stringify(entries).includes('owned-capability'),false);
 }finally{for(const[key,value]of [['AGBRIO_PI_SCOPE',previous.root],['AGBRIO_PI_CAP',previous.cap]]){if(value===undefined)delete process.env[key];else process.env[key]=value;}fs.rmSync(root,{recursive:true,force:true});}
});
test('canonical tool scope blocks traversal, credential names and symlink escapes',()=>{const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-guard-'));const other=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-outside-'));try{fs.writeFileSync(path.join(root,'code.txt'),'fixture');assert.equal(scopedPath(root,'code.txt'),path.join(root,'code.txt'));assert.equal(scopedPath(root,'new/code.txt',true),path.join(root,'new/code.txt'));for(const target of ['../outside','auth.json','.env','.git/config',other])assert.throws(()=>scopedPath(root,target,true));fs.symlinkSync(other,path.join(root,'linked'),'junction');assert.throws(()=>scopedPath(root,'linked/new.txt',true));}finally{fs.rmSync(root,{recursive:true,force:true});fs.rmSync(other,{recursive:true,force:true});}});

test('Pi-native @ and file URL normalization cannot escape admission',()=>{const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-norm-'));const other=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-ext-'));try{fs.writeFileSync(path.join(other,'plain.txt'),'outside');assert.throws(()=>scopedPath(root,'@'+path.join(other,'plain.txt')));assert.throws(()=>scopedPath(root,new URL('file:///'+path.join(other,'plain.txt').replaceAll('\\','/')).href));fs.mkdirSync(path.join(root,'.claude'));assert.throws(()=>safeSearch(root));}finally{fs.rmSync(root,{recursive:true,force:true});fs.rmSync(other,{recursive:true,force:true});}});

test('nonlocal file URLs and alternate streams are rejected before following a target',()=>{const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-pi-url-'));try{assert.throws(()=>scopedPath(root,'file://remote.invalid/share/file',true));assert.throws(()=>scopedPath(root,'allowed.txt:stream',true));}finally{fs.rmSync(root,{recursive:true,force:true});}});

test('only owner-allowlisted MCP tools pass while unknown tools and credential files stay blocked',async()=>{
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-mcp-guard-'));const previous={...process.env};const events={},entries=[];
 try{process.env.AGBRIO_PI_SCOPE=root;process.env.AGBRIO_PI_CAP='mcp-cap';process.env.AGBRIO_PI_WRITE='1';process.env.AGBRIO_PI_TERMINAL='1';process.env.AGBRIO_PI_ACCESS=JSON.stringify({roots:[{path:root,writable:true}],resources:{mcpServers:['unityMCP']}});guard({registerCommand:()=>{},on:(k,v)=>events[k]=v,appendEntry:(type,data)=>entries.push({type,data})});
 assert.equal(await events.tool_call({toolName:'mcp__unityMCP__read_editor',toolCallId:'one',input:{}},{mode:'rpc'}),undefined);assert.equal(entries.at(-1).data.authorization,'OWNER_MCP_ALLOWLIST');assert.equal((await events.tool_call({toolName:'mcp__foreign__write',input:{}},{mode:'rpc'})).block,true);assert.equal((await events.tool_call({toolName:'mcp__unityMCP__read_editor',input:{}},{mode:'interactive'})).block,true);assert.throws(()=>accessPath(root,'auth.json',false,[{path:root,writable:true,pinned:checkedRoot(root)}],{}));
 }finally{for(const k of Object.keys(process.env))if(k.startsWith('AGBRIO_PI_'))delete process.env[k];for(const[k,v]of Object.entries(previous))if(k.startsWith('AGBRIO_PI_'))process.env[k]=v;fs.rmSync(root,{recursive:true,force:true});}
});

test('capability metadata command reports actual native tools without starting a model',async()=>{const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-native-cap-'));const previous={...process.env};const commands={},entries=[];try{process.env.AGBRIO_PI_SCOPE=root;process.env.AGBRIO_PI_CAP='metadata-cap';guard({registerCommand:(k,v)=>commands[k]=v,on:()=>{},getAllTools:()=>[{name:'read'},{name:'mcp__inactive__read'}],getActiveTools:()=>['read'],appendEntry:(type,data)=>entries.push({type,data})});await commands['agbrio-capabilities'].handler('metadata-cap');assert.equal(entries[0].type,'agbrio.capabilities');assert.deepEqual(entries[0].data.tools,['read']);assert.equal(entries[0].data.toolSetSource,'ACTIVE_MODEL');assert.equal(entries[0].data.mcpReady,true);assert.equal(entries[0].data.runtimeId.length,64);await assert.rejects(()=>commands['agbrio-capabilities'].handler('wrong'));}finally{for(const k of Object.keys(process.env))if(k.startsWith('AGBRIO_PI_'))delete process.env[k];for(const[k,v]of Object.entries(previous))if(k.startsWith('AGBRIO_PI_'))process.env[k]=v;fs.rmSync(root,{recursive:true,force:true});}});

test('registered but inactive approved MCP is not reported model-ready',async()=>{
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'agbrio-inactive-cap-'));const previous={...process.env};const commands={},entries=[];
 try{
  process.env.AGBRIO_PI_SCOPE=root;process.env.AGBRIO_PI_CAP='inactive-cap';process.env.AGBRIO_PI_WRITE='1';process.env.AGBRIO_PI_TERMINAL='1';
  process.env.AGBRIO_PI_ACCESS=JSON.stringify({roots:[{path:root,writable:true}],resources:{mcpServers:['unityMCP']}});
  guard({registerCommand:(k,v)=>commands[k]=v,on:()=>{},getAllTools:()=>[{name:'read'},{name:'mcp__unityMCP__read_console'}],getActiveTools:()=>['read'],appendEntry:(type,data)=>entries.push({type,data})});
  await commands['agbrio-capabilities'].handler('inactive-cap');assert.deepEqual(entries[0].data.tools,['read']);assert.equal(entries[0].data.mcpReady,false);
 }finally{for(const k of Object.keys(process.env))if(k.startsWith('AGBRIO_PI_'))delete process.env[k];for(const[k,v]of Object.entries(previous))if(k.startsWith('AGBRIO_PI_'))process.env[k]=v;fs.rmSync(root,{recursive:true,force:true});}
});
