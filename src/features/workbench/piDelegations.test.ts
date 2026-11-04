import {expect,it} from 'vitest';
import {piDelegations} from './piDelegations';
import type {CodexWatch} from './CodexNotifications';
const watch=(threadId:string,cwd='D:\\work'):CodexWatch=>({threadId,cwd,label:'same title',enabled:true,generation:1,snapshot:{state:'IDLE',turnId:null,itemId:null,text:''},checkedAt:1,errorCode:null});
it('groups only the declared native parent and matching canonical project, never a title',()=>{
 const parent=watch('claude_code:parent'),other=watch('claude_code:other');
 const child={...watch('pi:child'),piParent:{provider:'CLAUDE_CODE',externalId:'parent',cwd:'D:/work/'}};
 const result=piDelegations([parent,other,child]);expect(result.children.get(parent.threadId)).toEqual([child]);expect(result.children.has(other.threadId)).toBe(false);expect(result.nested.has(child.threadId)).toBe(true);
 expect(piDelegations([other,child]).nested.size).toBe(0);
 expect(piDelegations([{...parent,cwd:'D:/different'},child]).nested.size).toBe(0);
});
it('uses owner-assigned current management without changing the original native parent',()=>{
 const old=watch('claude_code:old'),current=watch('manager');
 const child={...watch('pi:child'),piParent:{provider:'CLAUDE_CODE',externalId:'old',cwd:'D:/work'},management:{zoneId:'zone',managerId:'manager',managerCwd:'D:/work',revision:3}};
 const result=piDelegations([old,current,child]);expect(result.children.get('manager')).toEqual([child]);expect(result.children.has(old.threadId)).toBe(false);
 expect(child.piParent.externalId).toBe('old');expect(piDelegations([old,{...current,cwd:'D:/different'},child]).nested.size).toBe(0);
});
