import {expect,test,mock} from 'claude-code/testing';
test('native VM registers exact Codex wait, completes once and never repeats an unknown notice',async(driver,on)=>{
 mock.store(on);const clock=mock.clock(on,{now:1791777600000});let sourceState='RUNNING',submits=0,registrations=0;
 const sid='10000000-0000-4000-8000-000000000001',threadId='20000000-0000-4000-8000-000000000001',turnId='30000000-0000-4000-8000-000000000001';
 on('command.register',()=>({value:undefined}));on('tool.register',()=>{registrations++;return{value:undefined};});on('ui.log',()=>({value:undefined}));
 on('session.id',()=>({value:sid}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.308'}}));on('session.start',()=>({cwd:'D:\\fixture'}));
 on('http.fetch',($,e)=>({value:{ok:true,status:200,headers:{},text:e.url.endsWith('/codex-wait')?JSON.stringify({threadId,turnId,generation:1,state:sourceState}):'{}'}}));
 on('prompt.submit',($,e)=>{submits++;expect(e.text).toContain(threadId);expect(e.text).toContain('metadata only');expect(e.asUser).toBeUndefined();return{drop:'fictional test drop; never retry'};});
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});expect(registrations).toBe(1);
 const waiting=await driver.tool.call({tool:'mcp__agbrio-bridge__codex_wait',action:'wait',threadId,generation:1,turnId});expect(waiting.result).toContain('WAITING');
 await clock.advance(5000);expect(submits).toBe(0);sourceState='RESULT_READY';await clock.advance(5000);expect(submits).toBe(1);
 await clock.advance(5000);expect(submits).toBe(1);
 const status=await driver.tool.call({tool:'mcp__agbrio-bridge__codex_wait',action:'status'});expect(status.result).toContain('UNKNOWN');
});
test('native VM stores an independent exact notification row as DELIVERED',async(driver,on)=>{
 mock.store(on);const clock=mock.clock(on,{now:1791777600000});let submits=0;
 const sid='10000000-0000-4000-8000-000000000001',threadId='20000000-0000-4000-8000-000000000001',turnId='30000000-0000-4000-8000-000000000001',row='40000000-0000-4000-8000-000000000001';
 on('command.register',()=>({value:undefined}));on('tool.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));
 on('session.id',()=>({value:sid}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.308'}}));on('session.start',()=>({cwd:'D:\\fixture'}));on('session.append',($,e,next)=>next(e));
 on('http.fetch',($,e)=>({value:{ok:true,status:200,headers:{},text:e.url.endsWith('/codex-wait')?JSON.stringify({threadId,turnId,generation:1,state:'RESULT_READY'}):'{}'}}));
 on('prompt.submit',async($,e)=>{submits++;await driver.session.append({uuid:row,door:'prompt',origin:{kind:'plugin',name:'agbrio-bridge'},message:{type:'user',role:'user',content:[{type:'text',text:e.text}]}});return{text:e.text};});
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});
 await driver.tool.call({tool:'mcp__agbrio-bridge__codex_wait',action:'wait',threadId,generation:1,turnId});await clock.advance(5000);await clock.advance(5000);
 const status=await driver.tool.call({tool:'mcp__agbrio-bridge__codex_wait',action:'status'});expect(status.result).toContain('DELIVERED');expect(status.result).toContain(row);expect(submits).toBe(1);
});
