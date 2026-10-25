import {expect,test,mock} from 'claude-code/testing';
test('native VM submits one owner command and receipts the independently stored row',async(driver,on)=>{
 mock.store(on);const clock=mock.clock(on,{now:1791777600000});let submitCount=0;let offered=false;const receipts=[];
 const sid='10000000-0000-4000-8000-000000000001',rid='30000000-0000-4000-8000-000000000001',row='40000000-0000-4000-8000-000000000001';
 const text='Owner approved exact text';
 const hash=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text)))).map(v=>v.toString(16).padStart(2,'0')).join('');
 on('command.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));
 on('session.id',()=>({value:sid}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.295'}}));
 on('session.start',()=>({cwd:'D:\\fixture'}));
 on('session.append',($,e,next)=>next(e));
 on('prompt.submit',async($,e)=>{
  submitCount++;
  await driver.session.append({uuid:row,door:'prompt',origin:{kind:'plugin',name:'agbrio-bridge'},message:{type:'user',role:'user',content:[{type:'text',text:e.text}]}});
  return{text:e.text};
 });
 on('http.fetch',($,e)=>{
  const body=JSON.parse(e.init.body);
  if(e.url.endsWith('/receipts')){receipts.push(body);return{value:{ok:true,status:200,headers:{},text:'{}'}};}
  const command=body.kind==='HEARTBEAT'&&!offered?{requestId:rid,sessionId:sid,instanceId:body.instanceId,cwd:body.cwd,text,textSha256:hash}:null;
  if(command)offered=true;
  return{value:{ok:true,status:200,headers:{},text:JSON.stringify({accepted:true,command})}};
 });
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});await clock.advance(5000);await clock.advance(5000);
 expect(submitCount).toBe(1);expect(receipts.length).toBe(1);expect(receipts[0].status).toBe('SENT');expect(receipts[0].messageId).toBe(row);expect(receipts[0].textSha256).toBe(hash);
});
test('native VM observes full final bytes without sending or network',async ($,on)=>{
 on('command.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));
 const clock=mock.clock(on,{now:1791777600000});const posted=[];
 on('session.id',()=>({value:'10000000-0000-4000-8000-000000000001'}));
 on('session.cwd',()=>({value:'D:\\fixture'}));
 on('session.version',()=>({value:{version:'2.1.295'}}));
 on('session.start',()=>({cwd:'D:\\fixture'}));on('turn.start',($,e)=>({turnId:e.turnId}));on('turn.complete',()=>({text:''}));
 on('http.fetch',($,e)=>{posted.push(JSON.parse(e.init.body));return{value:{ok:true,status:200,headers:{},text:'{}'}};});
 await $.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});
 await clock.advance(5000);
 expect(posted[0].kind).toBe('SESSION_START');
 await $.turn.start({turnId:'30000000-0000-4000-8000-000000000001'});
 const answer='完整自然答复\n```text\n代码保持\n```';
 await $.turn.complete({turnId:'30000000-0000-4000-8000-000000000001',reason:'answer',answer,isAborted:false});
 await clock.advance(5000);
 const final=posted.find(e=>e.kind==='COMPLETE');
 expect(final.text).toBe(answer);expect(typeof final.completedAt).toBe('number');
});

test('native initialization failure is observable and never blocks or publishes',async ($,on)=>{
 const logs=[];let posted=0,passed=0;
 on('command.register',()=>({value:undefined}));on('ui.log',($,e)=>{logs.push(e.text);return{value:undefined};});
 on('session.id',()=>({value:'10000000-0000-4000-8000-000000000001'}));on('session.cwd',()=>({value:'D:\\fixture'}));
 on('session.version',()=>({deny:'PRIVATE_UNEXPECTED_FAILURE'}));on('session.start',()=>{passed++;return{cwd:'D:\\fixture'};});
 on('http.fetch',()=>{posted++;return{value:{ok:true,status:200,headers:{},text:'{}'}};});
 await $.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});
 const result=await $.command.run({command:'agbrio-status',args:''});
 expect(result.text).toContain('AGBRIO_VERSION_FAILED');expect(result.text).not.toContain('PRIVATE_UNEXPECTED_FAILURE');expect(passed).toBe(1);expect(posted).toBe(0);expect(logs.length).toBe(1);
});

test('native VM returns one-call decisions and preserves denials without prompt submission',async(driver,on)=>{
 mock.store(on);mock.clock(on,{now:1791777600000});let kind='',posts=0,submits=0;let nativeDecision='ask';
 on('command.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));
 on('session.id',()=>({value:'10000000-0000-4000-8000-000000000001'}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.304'}}));on('session.start',()=>({cwd:'D:\\fixture'}));
 on('classic.PreToolUse',()=>({}));on('tool.check',()=>({decision:nativeDecision}));on('prompt.submit',()=>{submits++;throw Error('NO_PROMPT_ALLOWED')});
 on('http.fetch',($,e)=>{const body=JSON.parse(e.init.body);posts++;let response={};
  if(e.url.endsWith('/owner-requests')){kind=body.kind;response={captured:true};}
  else if(e.url.endsWith('/poll'))response={status:'ANSWER',answer:kind==='QUESTION'?{action:'ANSWER',answers:{'Choose transport':'A'}}:{action:'ALLOW_ONCE'}};
  else if(e.url.endsWith('/receipt'))response={confirmed:true};
  return{value:{ok:true,status:200,headers:{},text:JSON.stringify(response)}};
 });
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});
 // AskUserQuestion classic hooks cannot be driven via a mocked tool.call:
 // the testing core is short-circuited. Their exact answer contract is covered
 // separately by owner-requests.test.mjs, not claimed as native decision UAT.
 const permission=await driver.tool.check({tool:'Bash',tool_use_id:'native-permission',command:'echo fictional'});expect(permission.decision).toBe('allow');expect(permission.updatedPermissions).toBeUndefined();
 const previous=posts;nativeDecision='deny';expect((await driver.tool.check({tool:'Bash',tool_use_id:'managed-deny',command:'echo fictional'})).decision).toBe('deny');expect(posts).toBe(previous);
 nativeDecision='ask';expect((await driver.tool.check({tool:'Bash',tool_use_id:'organization-ceiling',command:'echo fictional',ceiling:'ask'})).decision).toBe('ask');expect(posts).toBe(previous);expect(submits).toBe(0);
});
