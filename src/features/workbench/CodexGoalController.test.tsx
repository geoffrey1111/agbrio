import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {CodexGoalController} from './CodexGoalController';
import {displayedGoalSeconds} from './CodexGoalStatus';
import type {MobileCodexGoal} from '../../mobile/api';
import type {GoalControls,GoalControlReceipt,WatchChatApi} from './watchChatApi';
const goal:MobileCodexGoal={threadId:'goal-fixture',fingerprint:'old-fingerprint',objective:'Continue the original approved task',status:'blocked',timeUsedSeconds:20,tokensUsed:43,tokenBudget:null,updatedAt:1};
const controls:GoalControls={target:{threadId:goal.threadId,generation:-42,workstreamId:'fictional-bridge',bindingRevision:2,role:'EXECUTION',endpointId:'exact-recipient'},canPause:false,canResume:true,resumeRequiresOwnerAnswer:false,blockedReason:null};
const applied=(id:string):GoalControlReceipt=>({id,threadId:goal.threadId,payloadHash:'hash',status:'APPLIED',errorCode:null,result:{...goal,status:'active',fingerprint:'new-fingerprint',updatedAt:2}});
beforeEach(()=>sessionStorage.clear());afterEach(cleanup);
it('resumes the exact existing goal once and reflects the native response without a success acknowledgement',async()=>{
 let finish:(value:GoalControlReceipt)=>void=()=>{};let id='';const command=vi.fn(input=>{id=input.id as string;return new Promise<GoalControlReceipt>(resolve=>{finish=resolve;});});const refresh=vi.fn(async()=>{});
 render(<CodexGoalController goal={goal} controls={controls} api={{command} as unknown as WatchChatApi} refresh={refresh}/>);
 const resume=screen.getByRole('button',{name:'继续目标'});fireEvent.click(resume);fireEvent.click(resume);
 expect(command).toHaveBeenCalledTimes(1);expect(command.mock.calls[0][0]).toMatchObject({action:'GOAL_CONTROL',threadId:goal.threadId,operation:'RESUME_GOAL',input:{target:controls.target,expectedGoalFingerprint:goal.fingerprint},confirmed:true});
 await act(async()=>finish(applied(id)));await screen.findByText('进行中的目标');expect(refresh).toHaveBeenCalledOnce();expect(screen.queryByRole('button',{name:'知道了'})).toBeNull();
});
it('a lost control acknowledgement survives reopening and checks only the original receipt',async()=>{
 let id='';const command=vi.fn(async(input)=>{if(input.action==='GOAL_CONTROL'){id=input.id as string;throw Error('network lost');}if(input.action==='GOAL_RECEIPT')return applied(input.id as string);throw Error('unexpected action');});const props={goal,controls,api:{command} as unknown as WatchChatApi,refresh:vi.fn(async()=>{})};
 const view=render(<CodexGoalController {...props}/>);fireEvent.click(screen.getByRole('button',{name:'继续目标'}));await screen.findByRole('button',{name:'检查目标操作记录'});view.unmount();
 render(<CodexGoalController {...props}/>);expect(screen.getByRole('button',{name:'继续目标'})).toBeDisabled();fireEvent.click(screen.getByRole('button',{name:'检查目标操作记录'}));await waitFor(()=>expect(screen.getByText('进行中的目标')).toBeVisible());
 expect(command.mock.calls.filter(([input])=>input.action==='GOAL_CONTROL')).toHaveLength(1);expect(command.mock.calls.find(([input])=>input.action==='GOAL_RECEIPT')?.[0].id).toBe(id);
});
it('elapsed display uses native accounting, stops on pause, and freezes when the Host goes stale',()=>{
 const active=Object.freeze({...goal,status:'active'});expect(displayedGoalSeconds(active,10000,10000)).toBe(29);expect(displayedGoalSeconds(active,40000,10000)).toBe(44);expect(displayedGoalSeconds({...active,status:'paused'},40000,10000)).toBe(20);expect(active.timeUsedSeconds).toBe(20);
});
