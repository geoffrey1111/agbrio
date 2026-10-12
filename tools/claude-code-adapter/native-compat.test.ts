import {expect,test,mock} from 'claude-code/testing';
test('native VM observes full final bytes without sending or network',async ($,on)=>{
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
