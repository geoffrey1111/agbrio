import {render,screen,cleanup} from '@testing-library/react';
import {it,expect,vi,afterEach} from 'vitest';
import {TodoPage} from './TodoPage';
vi.mock('./sharedWatches',()=>({useWatches:()=>[[{threadId:'pi:owner-fixture',label:'API 账单测试',cwd:'D:/fixture',enabled:true,generation:1,snapshot:{state:'FAILED',turnId:'one',itemId:'error',text:'400 Your credit balance is too low to access the API.'},checkedAt:1,errorCode:'PI_PROVIDER_BILLING_REQUIRED'}]]}));
vi.mock('./ConversationHome',()=>({bridgeTone:()=> 'done',ConversationAvatar:()=> <span/>,when:()=> '刚刚'}));
vi.mock('../../i18n',()=>({useLanguage:()=> 'zh-CN',t:(key:string)=>key,tc:(key:string,...args:unknown[])=>key.replace(/\{(\d+)\}/g,(_,n)=>String(args[Number(n)]))}));
afterEach(cleanup);
it('renders provider billing failures first, with the reason, the upstream wording and the next step',()=>{
 render(<TodoPage bridges={[]} activity={{}} activityClock={1} onOpenBridge={()=>{}} onOpenWatch={()=>{}}/>);
 expect(screen.getByRole('heading',{name:'需要你处理 · 1'})).toBeInTheDocument();
 expect(screen.getByText('Pi 的上游 API 余额不足')).toBeInTheDocument();
 expect(screen.getByText('400 Your credit balance is too low to access the API.')).toBeInTheDocument();
 expect(screen.getByText(/去上游 API 平台充值/)).toBeInTheDocument();
 expect(screen.getByRole('button',{name:'知道了'})).toBeInTheDocument();
 expect(screen.queryByRole('heading',{name:/执行失败/})).toBeNull();
});
