import {expect,test,mock} from 'claude-code/testing';
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
