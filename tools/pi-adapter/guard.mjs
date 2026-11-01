// Fixed Agbrio-owned Pi extension. No automatic project extensions or MCP.
import fs from 'node:fs';import path from 'node:path';import os from 'node:os';import crypto from 'node:crypto';import {fileURLToPath} from 'node:url';
const pathEqual=(a,b)=>process.platform==='win32'?path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase():path.resolve(a)===path.resolve(b);
const sameFile=(a,b)=>{try{const x=fs.statSync(a,{bigint:true}),y=fs.statSync(b,{bigint:true});if(x.ino===0n||y.ino===0n)return pathEqual(fs.realpathSync(a),fs.realpathSync(b));return x.dev===y.dev&&x.ino===y.ino;}catch{return false;}};
const deniedNames=new Set(['.git','.pi','.claude','.codex','.ssh','.aws','.azure','.aiwr']);
const credentialName=name=>/^(?:(?:auth|credentials|\.credentials|secrets|private-session|browser-auth)\.json|\.claude\.json|\.mcp\.json|\.git-credentials|\.npmrc|\.netrc|id_rsa|id_ed25519)$|\.(?:key|pfx|p12|dpapi)$/i.test(name)||/^\.env(?:$|\.(?!example$))/i.test(name);
export function providerFailure(error){
 const s=String(error??'').toLowerCase();
 const status=Number(s.match(/^\s*([45][0-9]{2})\b/)?.[1]??s.match(/\b(?:http|status(?:\s+code)?)[\s"':=]+([45][0-9]{2})\b/)?.[1]);
 if(status===402||/credit balance|insufficient.credit|billing[ _-](?:limit|hard|error|required|disabled)|payment.required|insufficient.funds|balance.*(?:low|exhaust)|purchase credits/.test(s))return{code:'PI_PROVIDER_BILLING_REQUIRED',retryable:false};
 if(/insufficient.quota|quota|daily.limit|monthly.limit|spend.limit|usage.?limit|out.of.budget|available.balance/.test(s)&&!/(?:tokens|requests) per min/.test(s))return{code:'PI_PROVIDER_QUOTA_EXHAUSTED',retryable:false};
 if([401,403].includes(status)||/unauthori|authentication|permission|invalid.api.key/.test(s))return{code:'PI_PROVIDER_AUTHENTICATION_ERROR',retryable:false};
 if(status===413||/context|too.large/.test(s))return{code:'PI_PROVIDER_CONTEXT_LIMIT',retryable:false};
 if(/abort|cancel/.test(s))return{code:'PI_PROVIDER_CANCELLED',retryable:false};
 if(status===429||/rate.limit|tokens per min|requests per min/.test(s))return{code:'PI_PROVIDER_RATE_LIMIT',retryable:true};
 if(/connection error|econnreset|etimedout|timed out|socket hang up|fetch failed|tls|enotfound/.test(s))return{code:'PI_PROVIDER_CONNECTION_ERROR',retryable:!/certificate|cert_/.test(s)};
 if([500,502,503,504].includes(status)||/temporar|overloaded|capacity|internal.server.error|api_error/.test(s))return{code:'PI_PROVIDER_TEMPORARY_UNAVAILABLE',retryable:true};
 return{code:'PI_PROVIDER_ERROR',retryable:false};
}
export function recoveryDelay(error,attempt){
 const failure=providerFailure(error),base=failure.code==='PI_PROVIDER_RATE_LIMIT'?[15000,45000]:[5000,15000];
 const hint=String(error??'').match(/(?:try again in|retry.after[\s:=]+)\s*([0-9.]+)\s*(ms|s|seconds)?/i);
 const server=hint?Math.ceil(Number(hint[1])*(hint[2]==='ms'?1:1000)):0;
 if(!failure.retryable||attempt<1||attempt>2||!Number.isFinite(server)||server>60000)return null;
 return Math.max(base[attempt-1],server);
}
export function retryBoundary(event){
 if(event.outcome!=='error'||event.continue||event.context?.pendingMessages?.length)return null;
 const row=event.context?.contextEntries?.findLast(e=>e.sourceEntry?.type==='message');
 const message=row?.sourceEntry?.message;
 if(!row?.sourceEntry?.id||message?.role!=='assistant'||message.stopReason!=='error'||message.content?.length)return null;
 const error=String(message.errorMessage??'');
 return recoveryDelay(error,1)!==null?row.sourceEntry.id:null;
}
export function checkedRoot(root,expected){const absolute=path.resolve(root),prefix=path.parse(absolute).root;let current=prefix;for(const part of absolute.slice(prefix.length).split(path.sep).filter(Boolean)){current=path.join(current,part);if(fs.lstatSync(current).isSymbolicLink())throw Error('PI_SCOPE_ROOT_CHANGED');}const actual=fs.realpathSync(root);if(expected&&actual!==expected)throw Error('PI_SCOPE_ROOT_CHANGED');return actual;}
export function scopedPath(root,value,write=false,expectedRoot){
 const base=checkedRoot(root,expectedRoot);
 if(typeof value!=='string'||!value||value.length>8192||/[\0\r\n]/.test(value)||/^(?:\\\\|\/\/|[a-z]:[^\\/])/i.test(value))throw Error('PI_PATH_SCOPE_DENIED');
 let normalized=value.replace(/[\u00A0\u2000-\u200A\u202F\u205F\u3000]/g,' ').replace(/^@/,'');
 if(process.platform==='win32'){const shell=normalized.match(/^\/(?:mnt\/|cygdrive\/)?([a-z])(?:\/(.*))?$/i);if(shell)normalized=shell[1].toUpperCase()+':\\'+(shell[2]??'').replaceAll('/','\\');}
 if(normalized.startsWith('file://'))normalized=fileURLToPath(normalized);
 let expanded=normalized==='~'?os.homedir():/^~[\\/]/.test(normalized)?path.join(os.homedir(),normalized.slice(2)):normalized;
 const target=path.resolve(root,expanded);const relativeLexical=path.relative(root,target);if(relativeLexical==='..'||relativeLexical.startsWith('..'+path.sep)||path.isAbsolute(relativeLexical))throw Error('PI_PATH_SCOPE_DENIED');const parts=relativeLexical.split(path.sep);
 if(parts.some(p=>p.includes(':')||p==='..'||deniedNames.has(p.toLowerCase())||credentialName(p)))throw Error('PI_PATH_SCOPE_DENIED');
 let walk=root;for(const part of parts){walk=path.join(walk,part);try{if(fs.lstatSync(walk).isSymbolicLink())throw Error('PI_PATH_SCOPE_DENIED');}catch(error){if(error.code!=='ENOENT')throw error;}}
 let existing=target;while(!fs.existsSync(existing)){const next=path.dirname(existing);if(next===existing)throw Error('PI_PATH_SCOPE_DENIED');existing=next;}
 const canonical=fs.realpathSync(existing),relative=path.relative(base,canonical);
 if(relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative))throw Error('PI_PATH_SCOPE_DENIED');
 if(!write&&!fs.existsSync(target))throw Error('PI_PATH_UNAVAILABLE');return path.join(canonical,path.relative(existing,target));
}
export function safeSearch(root){
 const pending=[root];let seen=0;while(pending.length){const directory=pending.pop();for(const entry of fs.readdirSync(directory,{withFileTypes:true})){if(++seen>50000)throw Error('PI_SEARCH_SCOPE_TOO_LARGE');if(entry.isSymbolicLink()||deniedNames.has(entry.name.toLowerCase())||credentialName(entry.name))throw Error('PI_SEARCH_REQUIRES_NARROWER_PATH');if(entry.isDirectory())pending.push(path.join(directory,entry.name));}}
}
export function accessPath(cwd,value,write,grants,resources){
 if(typeof value!=='string'||!value||value.length>8192||/[\0\r\n]/.test(value)||/^(?:\\\\|\/\/|[a-z]:[^\\/])/i.test(value))throw Error('PI_PATH_SCOPE_DENIED');
 let normalized=typeof value==='string'?value.replace(/[\u00A0\u2000-\u200A\u202F\u205F\u3000]/g,' ').replace(/^@/,''):'';
 if(process.platform==='win32'){const shell=normalized.match(/^\/(?:mnt\/|cygdrive\/)?([a-z])(?:\/(.*))?$/i);if(shell)normalized=shell[1].toUpperCase()+':\\'+(shell[2]??'').replaceAll('/','\\');}
 if(normalized.startsWith('file://'))normalized=fileURLToPath(normalized);
 if(normalized==='~')normalized=os.homedir();else if(/^~[\\/]/.test(normalized))normalized=path.join(os.homedir(),normalized.slice(2));
 const target=path.resolve(cwd,normalized);
 for(const root of resources.protectedRoots??[]){const relative=path.relative(root,target);if(relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative))throw Error('PI_APPLICATION_STATE_PROTECTED');}
 if(!write){
  const instruction=resources.instructions?.find(v=>(pathEqual(v.path,target)||v.sourcePath&&pathEqual(v.sourcePath,target))&&sameFile(v.path,target));
  if(instruction){const root=path.dirname(instruction.path);return scopedPath(root,instruction.path,false,checkedRoot(root));}
  for(const root of [...(resources.skillRoots??[]),...(resources.skillFiles??[]).map(file=>path.dirname(file.path))]){const relative=path.relative(root,target);if(relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative))return scopedPath(root,target,false,checkedRoot(root));}
 }
 for(const root of resources.credentialRoots??[]){const relative=path.relative(root,target);if(relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative))throw Error('PI_CREDENTIAL_DIRECTORY_PROTECTED');}
 for(const grant of [...grants].sort((a,b)=>b.path.length-a.path.length)){
  const relative=path.relative(grant.path,target);if(relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative))continue;
  if(write&&!grant.writable)continue;
  return scopedPath(grant.path,target,write,grant.pinned);
 }
 throw Error('PI_PATH_SCOPE_DENIED');
}
export default function guard(pi){
 const root=process.env.AGBRIO_PI_SCOPE;const capability=process.env.AGBRIO_PI_CAP;
 if(!root||/[\u00A0\u2000-\u200A\u202F\u205F\u3000]/.test(root)||!path.isAbsolute(root)||!capability)throw Error('PI_SCOPE_REQUIRED');const pinnedRoot=checkedRoot(root);
 let policy=process.env.AGBRIO_PI_ACCESS?JSON.parse(process.env.AGBRIO_PI_ACCESS):null;if(process.env.AGBRIO_PI_ACCESS_FILE){const file=process.env.AGBRIO_PI_ACCESS_FILE;if(fs.lstatSync(file).isSymbolicLink()||fs.statSync(file).size>1024*1024)throw Error('PI_ACCESS_INVALID');const bytes=fs.readFileSync(file);if(crypto.createHash('sha256').update(bytes).digest('hex')!==process.env.AGBRIO_PI_ACCESS_DIGEST)throw Error('PI_ACCESS_CHANGED');policy=JSON.parse(bytes.toString('utf8'));}const resources=policy?.resources??{};const grants=(policy?.roots??[{path:root,writable:process.env.AGBRIO_PI_WRITE==='1'}]).map(v=>({...v,pinned:checkedRoot(v.path)}));
 function fatal(code){return new Promise(()=>{const exit=()=>process.exit(78);process.stdout.once('error',exit);process.stdout.write(JSON.stringify({type:'agbrio_guard_error',errorCode:code})+'\n',exit);setTimeout(exit,1000).unref();});}
 function verifySources(){for(const file of resources.integrationSources??[]){if(fs.lstatSync(file.path).isSymbolicLink()||crypto.createHash('sha256').update(fs.readFileSync(file.path)).digest('hex')!==file.sha256)throw Error('PI_INTEGRATION_SOURCE_CHANGED');}if(resources.appendFile){if(fs.lstatSync(resources.appendFile).isSymbolicLink()||crypto.createHash('sha256').update(fs.readFileSync(resources.appendFile)).digest('hex')!==resources.appendDigest)throw Error('PI_CONTEXT_CHANGED');}for(const rule of activeRules??resources.instructions??[]){checkedRoot(path.dirname(rule.path));if(fs.lstatSync(rule.path).isSymbolicLink()||rule.sourcePath&&!sameFile(rule.sourcePath,rule.path)||crypto.createHash('sha256').update(fs.readFileSync(rule.path)).digest('hex')!==rule.sha256)throw Error('PI_CONTEXT_CHANGED');}}
 const writable=process.env.AGBRIO_PI_WRITE==='1',terminal=process.env.AGBRIO_PI_TERMINAL==='1';let pending=null;const admitted=[];let logicalRun=null,runInputs=[],contextValidated=false,activeRules=null,recoveryAttempts=0;const skillReads=new Map();
 function fingerprint(text,images){const imageHash=crypto.createHash('sha256'),count=Buffer.alloc(4);count.writeUInt32BE((images??[]).length);imageHash.update(count);for(const image of images??[]){const mime=Buffer.from(image.mimeType,'utf8'),raw=Buffer.from(image.data,'base64'),mimeLength=Buffer.alloc(4),size=Buffer.alloc(8);mimeLength.writeUInt32BE(mime.length);size.writeBigUInt64BE(BigInt(raw.length));imageHash.update(mimeLength);imageHash.update(mime);imageHash.update(size);imageHash.update(raw);}return{textHash:crypto.createHash('sha256').update(text).digest('hex'),imagesHash:imageHash.digest('hex')};}
 pi.registerCommand('agbrio-control',{description:'Internal deterministic Agbrio intent receipt; not a model task',handler:async(args)=>{
  const fields=args.split(' ');if(fields.length!==5||fields[0]!==capability||!/^[a-zA-Z0-9_-]{1,128}$/.test(fields[1])||!fields.slice(2).every(v=>/^[a-f0-9]{64}$/.test(v)))throw Error('PI_CONTROL_DENIED');
  if(admitted.some(v=>v.requestId===fields[1]||(v.textHash===fields[3]&&v.imagesHash===fields[4])))throw Error('PI_IDENTICAL_INPUT_STILL_PENDING');
  pending={requestId:fields[1],payloadHash:fields[2],textHash:fields[3],imagesHash:fields[4]};pi.appendEntry('agbrio.intent',{requestId:pending.requestId,payloadHash:pending.payloadHash});
 }});
 pi.registerCommand('agbrio-forget-control',{description:'Internal rejected-input metadata cleanup',handler:async(args)=>{const fields=args.split(' ');if(fields.length!==3||fields[0]!==capability)throw Error('PI_CONTROL_DENIED');for(let i=admitted.length-1;i>=0;i--)if(admitted[i].requestId===fields[1]&&admitted[i].payloadHash===fields[2])admitted.splice(i,1);if(pending?.requestId===fields[1]&&pending.payloadHash===fields[2])pending=null;pi.appendEntry('agbrio.not_accepted',{requestId:fields[1],payloadHash:fields[2]});}});
 pi.registerCommand('agbrio-clear-control',{description:'Internal cancelled queue receipt cleanup',handler:async(args)=>{if(args!==capability)throw Error('PI_CONTROL_DENIED');pending=null;admitted.length=0;pi.appendEntry('agbrio.queue_cleared',{});}});
 pi.on('before_agent_start',async(event)=>{
  if(!policy)return;
  const digest=value=>crypto.createHash('sha256').update(value).digest('hex');
  try {
   const options=event.systemPromptOptions;
   if(resources.appendFile){if(fs.lstatSync(resources.appendFile).isSymbolicLink()||digest(fs.readFileSync(resources.appendFile))!==resources.appendDigest)throw Error('PI_CONTEXT_CHANGED');if(!options||options.forceSystemPrompt||!options.appendSystemPrompt?.includes(fs.readFileSync(resources.appendFile,'utf8').replace(/^\uFEFF/,'').trim()))throw Error('PI_CONTEXT_NOT_LOADED');}
   const loadedInstructions=(resources.instructions??[]).filter(rule=>rule.via==='append');
   for(const file of options?.contextFiles??[]){const rule=resources.instructions?.find(rule=>rule.via!=='append'&&sameFile(rule.path,file.path));if(!rule||digest(file.content)!==digest(fs.readFileSync(rule.path,'utf8').replace(/^\uFEFF/,'')))throw Error('PI_CONTEXT_NOT_LOADED');loadedInstructions.push(rule);}
   if((resources.instructions??[]).some(rule=>rule.required&&rule.via!=='append'&&!loadedInstructions.some(loaded=>sameFile(loaded.path,rule.path))))throw Error('PI_CONTEXT_NOT_LOADED');
   activeRules=loadedInstructions;verifySources();
   const nativeSkills=(options?.skills??[]).map(skill=>({name:skill.name,path:path.resolve(skill.filePath),descriptorDigest:digest(JSON.stringify({name:skill.name,description:skill.description,filePath:skill.filePath}))}));
   if(nativeSkills.some(skill=>!resources.skillFiles?.some(file=>path.resolve(file.path)===skill.path)))throw Error('PI_SKILL_SOURCE_UNAPPROVED');
   if(!runInputs.length&&!admitted.length)throw Error('PI_RUN_NOT_DELEGATED');
   contextValidated=true;
   pi.appendEntry('agbrio.resources',{runtimeId:digest(capability),instructions:loadedInstructions,skillRoots:resources.skillRoots??[],nativeSkills,authorization:'OWNER_WORKSPACE_POLICY',terminal});
  }catch(error){const code=/^PI_[A-Z_]+$/.test(error?.message??'')?error.message:'PI_CONTEXT_UNAVAILABLE';return fatal(code);}
 });
 pi.on('input',async(event)=>{
  if(event.source!=='rpc')return;
  const actual=fingerprint(event.text,event.images);
  if(!pending||pending.textHash!==actual.textHash||pending.imagesHash!==actual.imagesHash)return{action:'handled'};
  pi.appendEntry('agbrio.input',{requestId:pending.requestId,payloadHash:pending.payloadHash});admitted.push(pending);pending=null;return{action:'continue'};
 });
 pi.on('message_start',async(event)=>{const message=event.message;if(message?.role!=='user')return;const content=message.content;const text=typeof content==='string'?content:(content??[]).filter(b=>b.type==='text').map(b=>b.text).join('\n');const images=typeof content==='string'?[]:(content??[]).filter(b=>b.type==='image');const actual=fingerprint(text,images);const index=admitted.findIndex(v=>v.textHash===actual.textHash&&v.imagesHash===actual.imagesHash);if(index<0)return;const [intent]=admitted.splice(index,1);logicalRun??=crypto.randomUUID();runInputs.push({requestId:intent.requestId,payloadHash:intent.payloadHash});pi.appendEntry('agbrio.consumed',{requestId:intent.requestId,payloadHash:intent.payloadHash});});
 pi.on('agent_start',async()=>{logicalRun??=crypto.randomUUID();});
 pi.on('before_provider_request',async(event)=>{let bytes=null;try{bytes=Buffer.byteLength(JSON.stringify(event.payload),'utf8');}catch{}pi.appendEntry('agbrio.request_size',{runId:logicalRun,bytes});});
 pi.on('after_provider_response',async(event)=>{pi.appendEntry('agbrio.response_status',{runId:logicalRun,status:Number.isInteger(event.status)?event.status:null});});
 pi.on('before_provider_headers',async()=>{try{if(!pending&&!admitted.length&&!runInputs.length)throw Error('PI_RUN_NOT_DELEGATED');if(!contextValidated)verifySources();pi.appendEntry('agbrio.model_request',{runtimeId:crypto.createHash('sha256').update(capability).digest('hex'),runId:logicalRun});}catch(error){return fatal(/^PI_[A-Z_]+$/.test(error?.message??'')?error.message:'PI_CONTEXT_UNAVAILABLE');}});
 pi.on('agent_before_settle',async(event,ctx)=>{
  if(pi.getSettings?.().retry?.enabled===true)return;
  const target=retryBoundary(event);if(!target||!logicalRun||!runInputs.length||recoveryAttempts>=2||ctx.signal?.aborted||ctx.hasPendingMessages())return;
  const error=event.context.contextEntries.findLast(e=>e.sourceEntry?.id===target)?.sourceEntry?.message?.errorMessage;
  const attempt=recoveryAttempts+1,delayMs=recoveryDelay(error,attempt);if(delayMs===null)return;recoveryAttempts=attempt;verifySources();
  pi.appendEntry('agbrio.retry',{runId:logicalRun,nativeEntryId:target,attempt,maxRetries:2,delayMs,phase:'WAITING',errorCode:providerFailure(error).code});
  await new Promise(resolve=>{const timer=setTimeout(done,delayMs);function done(){clearTimeout(timer);ctx.signal?.removeEventListener('abort',done);resolve();}ctx.signal?.addEventListener('abort',done,{once:true});});
  if(ctx.signal?.aborted||ctx.hasPendingMessages())return;verifySources();
  // Native boundary continuation retains completed tools and the original input.
  // Only the definitive empty failed response is omitted from model context;
  // its append-only history remains intact. No prompt/command is resubmitted.
  return {entries:[{type:'context_edit',targetId:target,replacement:null}],continue:true};
 });
 pi.on('agent_settled',async(event,ctx)=>{pi.appendEntry('agbrio.settled',{runId:logicalRun??crypto.randomUUID(),leafId:ctx.sessionManager.getLeafId(),aborted:event.aborted,requests:runInputs,retryAttempts:recoveryAttempts,retryPolicy:pi.getSettings?.().retry?.enabled===true?'NATIVE_INHERITED':'BOUNDED_EMPTY_RESPONSE',remaining:admitted.map(v=>({requestId:v.requestId,payloadHash:v.payloadHash}))});logicalRun=null;runInputs=[];contextValidated=false;recoveryAttempts=0;});
 pi.on('tool_result',async(event)=>{const read=skillReads.get(event.toolCallId);skillReads.delete(event.toolCallId);if(read&&event.isError===false){try{if(crypto.createHash('sha256').update(fs.readFileSync(read.path)).digest('hex')===read.sha256)pi.appendEntry('agbrio.skill_read',{...read,toolCallId:event.toolCallId,runtimeId:crypto.createHash('sha256').update(capability).digest('hex')});}catch{}}});
 pi.on('tool_call',async(event,ctx)=>{
  if((resources.mcpServers??[]).some(name=>event.toolName.startsWith(`mcp__${name}__`))){if(!terminal||!writable||ctx.mode!=='rpc')return{block:true,reason:'PI_MCP_WORKSPACE_POLICY_REQUIRED'};try{verifySources();}catch{return{block:true,reason:'PI_INTEGRATION_SOURCE_CHANGED'};}pi.appendEntry('agbrio.tool_authorized',{toolCallId:event.toolCallId,toolName:event.toolName,authorization:'OWNER_MCP_ALLOWLIST'});return undefined;}
  if(!['read','edit','write','grep','find','ls','powershell'].includes(event.toolName))return{block:true,reason:'PI_TOOL_NOT_GRANTED'};
  if(event.toolName==='powershell'){
   if(!terminal||!policy||ctx.mode!=='rpc')return{block:true,reason:'PI_TERMINAL_WORKSPACE_POLICY_REQUIRED'};
   const command=event.input?.command;if(typeof command!=='string'||!command||command.length>16000||/auth\.json|\.credentials\.json|AGBRIO_PI_/i.test(command))return{block:true,reason:'PI_TERMINAL_SCOPE_DENIED'};
   const payloadHash=crypto.createHash('sha256').update(JSON.stringify({directory:root,command})).digest('hex');
   pi.appendEntry('agbrio.tool_authorized',{toolCallId:event.toolCallId,payloadHash,authorization:'OWNER_WORKSPACE_POLICY'});
   return undefined;
  }
  if(['edit','write'].includes(event.toolName)&&!writable)return{block:true,reason:'PI_READ_ONLY_SCOPE'};
  try{const effective=accessPath(root,event.input?.path??root,['edit','write'].includes(event.toolName),grants,resources);if(['edit','write'].includes(event.toolName)&&/(?:^|[\\/])(?:AGENTS\.md|CLAUDE\.md|\.agents)(?:$|[\\/])/i.test(effective))return{block:true,reason:'PI_AUTHORITY_WRITE_NOT_GRANTED'};if(['grep','find'].includes(event.toolName)&&fs.statSync(effective).isDirectory())safeSearch(effective);if(event.toolName==='read'&&resources.skillFiles?.some(file=>sameFile(file.path,effective)))skillReads.set(event.toolCallId,{path:effective,sha256:crypto.createHash('sha256').update(fs.readFileSync(effective)).digest('hex')});event.input.path=effective;}catch(error){return{block:true,reason:error?.message==='PI_SEARCH_REQUIRES_NARROWER_PATH'?'PI_SEARCH_REQUIRES_NARROWER_PATH':'PI_PATH_SCOPE_DENIED'};}
 });
 // Registered last: discovery cannot report readiness after a partially loaded
 // extension. Agbrio checks this exact CLI resource before sending any prompt.
 for(const key of Object.keys(process.env))if(key.startsWith('AGBRIO_PI_'))delete process.env[key];
 pi.registerCommand('agbrio-capabilities',{description:'Internal read-only native capability receipt',handler:async(args)=>{
  if(args!==capability)throw Error('PI_CONTROL_DENIED');
  const names=resources.mcpServers??[];let tools=[];
  // Registration is not model exposure: --tools may keep MCP tools registered
  // for internal discovery while preventing direct model calls.
  for(let i=0;i<40;i++){tools=pi.getActiveTools();if(names.every(n=>tools.some(t=>t.startsWith(`mcp__${n}__`))))break;await new Promise(r=>setTimeout(r,200));}
  pi.appendEntry('agbrio.capabilities',{runtimeId:crypto.createHash('sha256').update(capability).digest('hex'),toolSetSource:'ACTIVE_MODEL',tools,mcpServers:names,mcpReady:names.every(n=>tools.some(t=>t.startsWith(`mcp__${n}__`))),recoveryPolicy:{mode:pi.getSettings?.().retry?.enabled===true?'NATIVE_INHERITED':'BOUNDED_EMPTY_RESPONSE',maxAdditionalRetries:pi.getSettings?.().retry?.enabled===true?0:2,delaysMs:[5000,15000],rateLimitDelaysMs:[15000,45000],maxDelayMs:60000,requestOnly:true}});
 }});
 pi.registerCommand('agbrio-guard-ready',{description:'Agbrio guard protocol 1',handler:async()=>{throw Error('PI_GUARD_METADATA_ONLY');}});
}
