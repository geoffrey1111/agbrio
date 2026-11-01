import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {pathToFileURL} from 'node:url';
import guard,{retryBoundary,providerFailure,recoveryDelay} from './guard.mjs';

const boundary=(error,content=[])=>({outcome:'error',continue:false,context:{pendingMessages:[],contextEntries:[{sourceEntry:{type:'message',id:'native-error',message:{role:'assistant',stopReason:'error',errorMessage:error,content}}}]}});
test('bounded recovery accepts only definitive empty transient responses',()=>{
 assert.equal(retryBoundary(boundary('Connection error.')),'native-error');
 for(const error of ['429 rate limit','500 internal server error','502 Bad Gateway','503 overloaded'])assert.equal(retryBoundary(boundary(error)),'native-error');
 for(const error of ['400 credit balance is too low','429 insufficient_quota','unknown provider error','429 rate limit, try again in 120s'])assert.equal(retryBoundary(boundary(error)),null);
 assert.equal(providerFailure('credit balance is too low').code,'PI_PROVIDER_BILLING_REQUIRED');
 assert.equal(providerFailure('Rate limit reached on tokens per min (TPM): Limit 1000000, Used 920014, Requested 141393. You can increase your rate limit by adding a payment method at https://platform.openai.com/account/billing.').code,'PI_PROVIDER_RATE_LIMIT');
 assert.equal(recoveryDelay('429 rate limit',1),15000);assert.equal(recoveryDelay('429 rate limit',2),45000);assert.equal(recoveryDelay('429 try again in 55s',1),55000);assert.equal(recoveryDelay('Connection error',3),null);
 for(const error of ['context too long','413 request too large','401 unauthorized','403 permission denied','429 quota','cancelled'])assert.equal(retryBoundary(boundary(error)),null);
 assert.equal(retryBoundary(boundary('Connection error.',[{type:'text',text:'partial'}])),null);
 assert.equal(retryBoundary({...boundary('Connection error.'),outcome:'aborted'}),null);
 const pending=boundary('Connection error.');pending.context.pendingMessages=[{}];assert.equal(retryBoundary(pending),null);
});

test('official Pi boundary retries without replaying user input or tools',{skip:process.env.AGBRIO_PI_SDK_QA!=='1'},async()=>{
 const pkg=path.join(process.env.APPDATA,'npm/node_modules/@earendil-works/pi-coding-agent');
 const {createAgentSession}=await import(pathToFileURL(path.join(pkg,'dist/core/sdk.js')));
 const {ModelRuntime}=await import(pathToFileURL(path.join(pkg,'dist/core/model-runtime.js')));
 const {SettingsManager}=await import(pathToFileURL(path.join(pkg,'dist/core/settings-manager.js')));
 const {SessionManager}=await import(pathToFileURL(path.join(pkg,'dist/core/session-manager.js')));
 const {DefaultResourceLoader}=await import(pathToFileURL(path.join(pkg,'dist/core/resource-loader.js')));
 const {AssistantMessageEventStream}=await import(pathToFileURL(path.join(pkg,'node_modules/@earendil-works/pi-ai/dist/utils/event-stream.js')));
 const realTimeout=globalThis.setTimeout;globalThis.setTimeout=(fn,ms,...args)=>realTimeout(fn,[5000,15000,45000,60000].includes(ms)?1:ms,...args);
 try{for(const [failureCount,error,expected,withTool=false,cancel=false,nativeRetry=false] of [[2,'Connection error.',3],[9,'Connection error.',3],[2,'429 rate limit reached',3],[9,'429 rate limit reached',3],[2,'502 Bad Gateway',3],[9,'credit balance is too low',1],[9,'429 insufficient_quota',1],[9,'unknown provider error',1],[9,'context too long',1],[9,'401 unauthorized',1],[2,'Connection error.',4,true],[9,'Connection error.',1,false,true],[9,'Connection error.',2,false,false,true],[9,'credit balance is too low',1,false,false,true],[9,'429 insufficient_quota',1,false,false,true]]){
  const root=fs.mkdtempSync(path.join(process.env.AGBRIO_PI_QA_ROOT??os.tmpdir(),'agbrio-retry-native-'));let session;
  try{
   process.env.AGBRIO_PI_SCOPE=root;process.env.AGBRIO_PI_CAP='fixture-cap';
   const runtime=await ModelRuntime.create({authPath:path.join(root,'empty-auth.json'),modelsPath:null,allowModelNetwork:false,refreshOnCreate:false});
   runtime.registerProvider('agbrio-fixture',{api:'openai-completions',apiKey:'fixture-not-secret',baseUrl:'http://127.0.0.1:9',models:[{id:'fixture',name:'fixture',reasoning:false,input:['text'],contextWindow:10000,maxTokens:100,cost:{input:0,output:0,cacheRead:0,cacheWrite:0}}]});
   const settings=SettingsManager.inMemory({retry:{enabled:nativeRetry,maxRetries:1,baseDelayMs:1,maxAgentDelayMs:1},compaction:{enabled:false}});
   const loader=new DefaultResourceLoader({cwd:root,agentDir:root,settingsManager:settings,extensionFactories:[guard],noExtensions:true,disabledBuiltinExtensions:['mcp'],noSkills:true,noContextFiles:true,noThemes:true,noPromptTemplates:true});await loader.reload();
   const manager=SessionManager.inMemory(root);({session}=await createAgentSession({cwd:root,agentDir:root,modelRuntime:runtime,model:runtime.getModel('agbrio-fixture','fixture'),settingsManager:settings,sessionManager:manager,resourceLoader:loader,tools:withTool?['read']:[]}));
   let calls=0,allowFailures=true;const injectedStream=()=>{const stream=new AssistantMessageEventStream();const tool=withTool&&++calls===1;if(!withTool)calls++;const failed=allowFailures&&!tool&&calls<=failureCount+(withTool?1:0);const m={role:'assistant',api:'openai-completions',provider:'agbrio-fixture',model:'fixture',content:tool?[{type:'toolCall',id:'fixture-read',name:'read',arguments:{path:path.join(root,'fixture.txt')}}]:failed?[]:[{type:'text',text:'OK'}],stopReason:tool?'toolUse':failed?'error':'stop',errorMessage:failed?error:undefined,timestamp:Date.now(),usage:{input:0,output:0,cacheRead:0,cacheWrite:0,totalTokens:0,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}}};queueMicrotask(()=>{stream.push({type:'start',partial:m});stream.push(failed?{type:'error',reason:'error',error:m}:{type:'done',reason:tool?'toolUse':'stop',message:m});if(cancel)realTimeout(()=>session.abort(),0);});return stream;};runtime.registerProvider('agbrio-fixture',{api:'openai-completions',streamSimple:injectedStream});
   fs.writeFileSync(path.join(root,'fixture.txt'),'OWNED_READ_ONLY_FIXTURE');const text='fixture';await session.prompt(`/agbrio-control fixture-cap input1 ${'a'.repeat(64)} ${crypto.createHash('sha256').update(text).digest('hex')} ${crypto.createHash('sha256').update(Buffer.alloc(4)).digest('hex')}`,{source:'rpc'});
   await session.prompt(text,{source:'rpc'});
   const entries=manager.getEntries();assert.equal(calls,expected);assert.equal(entries.filter(e=>e.type==='message'&&e.message.role==='user').length,1);if(!cancel&&!nativeRetry)assert.equal(entries.filter(e=>e.type==='context_edit').length,expected-1-(withTool?1:0));assert.equal(entries.filter(e=>e.type==='custom'&&e.customType==='agbrio.settled').length,1);assert.equal(entries.filter(e=>e.type==='message'&&e.message.role==='toolResult').length,withTool?1:0);assert.equal(settings.getRetrySettings().enabled,nativeRetry);
   if(failureCount===9&&error==='Connection error.'&&!cancel&&!nativeRetry){
    const nativeId=manager.getSessionId();allowFailures=false;const follow='continue';
    await session.prompt(`/agbrio-control fixture-cap input2 ${'b'.repeat(64)} ${crypto.createHash('sha256').update(follow).digest('hex')} ${crypto.createHash('sha256').update(Buffer.alloc(4)).digest('hex')}`,{source:'rpc'});await session.prompt(follow,{source:'rpc'});
    assert.equal(manager.getSessionId(),nativeId);assert.equal(calls,expected+1);
    const next=manager.getEntries();assert.equal(next.filter(e=>e.type==='message'&&e.message.role==='user').length,2);assert.equal(next.filter(e=>e.type==='custom'&&e.customType==='agbrio.settled').length,2);assert.equal(next.findLast(e=>e.type==='message'&&e.message.role==='assistant').message.stopReason,'stop');
   }

  }finally{session?.dispose();fs.rmSync(root,{recursive:true,force:true});}
 }}finally{globalThis.setTimeout=realTimeout;}
});
