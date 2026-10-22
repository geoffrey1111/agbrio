import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {ClaudeDecisionCards} from './ClaudeDecisionCards';
import type {ClaudeOwnerRequest,WatchChatApi} from './watchChatApi';
afterEach(cleanup);
const question:ClaudeOwnerRequest={id:'request-one',sessionId:'native-child',kind:'QUESTION',status:'PENDING',revision:1,payload:{questions:[{question:'选择方案',header:'方案',multiSelect:true,options:[{label:'A',description:'第一项'},{label:'B',description:'第二项'}]}]}};
function setup(request=question,failure=false){const command=vi.fn(async(_input:Record<string,unknown>)=>{if(failure)throw Error('offline');return {accepted:true}});const refresh=vi.fn(async()=>{});render(<ClaudeDecisionCards requests={[request]} threadId="claude_code:root" generation={-12} childId="native-child" api={{command} as unknown as WatchChatApi} refresh={refresh}/>);return {command,refresh};}
it('submits native question keys and all selected answers once, without sending a prompt',async()=>{
 const {command}=setup();expect(screen.getByRole('button',{name:'提交选择'})).toBeDisabled();
 fireEvent.click(screen.getByLabelText(/A第一项/));fireEvent.click(screen.getByLabelText(/B第二项/));fireEvent.change(screen.getByRole('textbox'),{target:{value:'补充内容'}});fireEvent.click(screen.getByRole('button',{name:'提交选择'}));
 await screen.findByRole('status');expect(command).toHaveBeenCalledExactlyOnceWith({action:'CLAUDE_DECISION',threadId:'claude_code:root',generation:-12,childId:'native-child',id:'request-one',revision:1,answer:{action:'ANSWER',answers:{'选择方案':'A, B, 补充内容'}}});
 fireEvent.click(screen.getByRole('button',{name:'刷新状态'}));expect(command).toHaveBeenCalledTimes(1);
});
it('an uncertain response locks the decision and only permits reading the status',async()=>{
 const {command}=setup(question,true);fireEvent.click(screen.getByLabelText(/A第一项/));fireEvent.click(screen.getByRole('button',{name:'提交选择'}));
 await screen.findByText('提交结果尚未确认，不会自动重发。');expect(screen.queryByRole('button',{name:'提交选择'})).toBeNull();fireEvent.click(screen.getByRole('button',{name:'刷新状态'}));expect(command).toHaveBeenCalledTimes(1);
});
it('permission actions remain one-call decisions and expose the exact requested input',async()=>{
 const {command}=setup({...question,kind:'PERMISSION',payload:{tool:'Bash',input:{command:'fictional command'}}});expect(screen.getByText(/fictional command/)).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'仅允许这次'}));await waitFor(()=>expect(command).toHaveBeenCalledTimes(1));expect(command.mock.calls[0][0]).toMatchObject({answer:{action:'ALLOW_ONCE'}});
});
it('can hand a request back to its original native card',async()=>{const {command}=setup();fireEvent.click(screen.getByRole('button',{name:'在 Claude 中处理'}));await waitFor(()=>expect(command).toHaveBeenCalledTimes(1));expect(command.mock.calls[0][0]).toMatchObject({answer:{action:'NATIVE'}});});
