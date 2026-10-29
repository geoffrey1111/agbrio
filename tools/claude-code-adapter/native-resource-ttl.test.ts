import {expect,test,mock} from 'claude-code/testing';
test('native resource prompt guard drops expired own notices but preserves user and task prompts',async(driver,on)=>{
 mock.store(on);const clock=mock.clock(on,{now:100000});let calls=0,offered=false;const text='Agbrio 本机资源告警（只读，提醒）\n采样时间：1970-01-01T00:01:40.000Z\nmetrics';
 on('command.register',()=>({value:undefined}));on('tool.register',()=>({value:undefined}));on('ui.log',()=>({value:undefined}));on('session.id',()=>({value:'10000000-0000-4000-8000-000000000001'}));on('session.cwd',()=>({value:'D:\\fixture'}));on('session.version',()=>({value:{version:'2.1.309'}}));on('session.start',()=>({cwd:'D:\\fixture'}));on('http.fetch',($,e)=>{const resource=e.url.endsWith('/resource-notice');const notice=resource&&!offered?{id:'resource:20000000-0000-4000-8000-000000000001',text,observedAt:100000}:null;if(resource)offered=true;return{value:{ok:true,status:200,headers:{},text:JSON.stringify({notice,subscriptions:[],settled:[]})}};});on('prompt.submit',($,e)=>{calls++;return{text:e.text};});
 await driver.session.start({surface:'desktop',isInteractive:true,cwd:'D:\\fixture'});
 const own={text,origin:{kind:'plugin',name:'agbrio-bridge'}};
 await clock.advance(5000);expect(calls).toBe(1);await clock.advance(115000);await driver.prompt.submit(own as any);expect(calls).toBe(2);
 await clock.advance(1);const result=await driver.prompt.submit(own as any);expect(result.drop).toBe('AGBRIO_RESOURCE_NOTICE_EXPIRED');expect(calls).toBe(2);
 await driver.prompt.submit({...own,origin:{kind:'user'}} as any);expect(calls).toBe(3);
 await driver.prompt.submit({text:'Agbrio Codex task notice (metadata only)\nkind: QUESTION',origin:{kind:'plugin',name:'agbrio-bridge'}} as any);expect(calls).toBe(4);
});
