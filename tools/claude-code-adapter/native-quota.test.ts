import {expect,test,mock} from 'claude-code/testing';
test('Desktop session usage reaches only the local quota publisher without model or login calls',async(driver,on)=>{
 const clock=mock.clock(on,{now:1791777600000});const posted=[];let prompts=0;
 on('command.register',()=>({value:undefined}));on('tool.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));
 on('session.id',()=>({value:'10000000-0000-4000-8000-000000000001'}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.309'}}));on('session.start',()=>({cwd:'D:\\fixture'}));
 on('session.usage',()=>({value:{startedAt:1791777600000,context:{tokens:100,window:200000,percent:0.05},rateLimits:[{kind:'five_hour',percentUsed:27,resetsAt:'2026-10-29T12:20:00Z'},{kind:'seven_day',percentUsed:21}]}}));
 on('prompt.submit',()=>{prompts++;return{drop:'test never submits'};});
 on('http.fetch',($,e)=>{if(e.url.endsWith('/usage'))posted.push(JSON.parse(e.init.body));return{value:{ok:true,status:200,headers:{},text:'{}'}};});
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});await clock.advance(5000);
 expect(posted.length).toBe(1);expect(posted[0].windows.map(v=>v.percentUsed)).toEqual([27,21]);expect(posted[0].context).toBeUndefined();expect(posted[0].cost).toBeUndefined();expect(prompts).toBe(0);
 await clock.advance(5000);expect(posted.length).toBe(1);
});
