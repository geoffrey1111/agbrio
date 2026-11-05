import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {describe,it,expect,vi,afterEach} from 'vitest';
import {LocalSafetySettings,localSafetyApi} from './LocalSafetySettings';
vi.mock('./ResourceMonitorSettings',()=>({ResourceMonitorSettings:()=>null}));
afterEach(cleanup);
describe('owner safety settings',()=>{
 it('defaults recovery to all watched Codex threads and leaves the manager unbound until saved',async()=>{
  const initial={settings:{recoveryEnabled:true,alertTarget:null},collectorAvailable:true,recent:[],targets:[{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager',label:'管理对话'}]};
  const api={read:vi.fn().mockResolvedValue(initial),watches:vi.fn().mockResolvedValue([{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager',enabled:true,label:'管理对话'}]),save:vi.fn().mockImplementation(settings=>Promise.resolve({...initial,settings}))} as unknown as typeof localSafetyApi;
  render(<LocalSafetySettings api={api}/>);
  expect(await screen.findByLabelText('自动恢复 Codex 模型故障')).toBeChecked();expect(screen.getByText(/没有 Goal 的普通对话仍需在 Codex 中重试/)).toBeVisible();expect(screen.getByRole('button',{name:'选择接收对话'})).toBeVisible();expect(api.save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'选择接收对话'}));fireEvent.click(await screen.findByRole('button',{name:'选择 管理对话'}));fireEvent.click(screen.getByRole('button',{name:'保存设置'}));
  await waitFor(()=>expect(api.save).toHaveBeenCalledWith({recoveryEnabled:true,alertTarget:{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager'},fallbackTargets:[]}));expect(await screen.findByText('已保存')).toBeInTheDocument();
 });
 it('shows failed read without implying protection is active',async()=>{const api={read:vi.fn().mockRejectedValue(Error('offline')),watches:vi.fn().mockResolvedValue([])} as unknown as typeof localSafetyApi;render(<LocalSafetySettings api={api}/>);expect(await screen.findByRole('alert')).toHaveTextContent('暂时无法读取保护设置');expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();});
 it('adds ordered backups, promotes one with arrows and removes without losing the remaining bindings',async()=>{
  const a={threadId:'claude_code:primary',generation:1,cwd:'D:\\primary',label:'主 Claude'},b={threadId:'codex-backup',generation:2,cwd:'D:\\backup',label:'备用 Codex'};
  const initial={settings:{recoveryEnabled:true,alertTarget:null,fallbackTargets:[]},collectorAvailable:true,recent:[],targets:[a,b]};const api={read:vi.fn().mockResolvedValue(initial),save:vi.fn().mockImplementation(settings=>Promise.resolve({...initial,settings}))} as typeof localSafetyApi;
  render(<LocalSafetySettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'选择接收对话'}));fireEvent.click(await screen.findByRole('button',{name:'选择 主 Claude'}));fireEvent.click(screen.getByRole('button',{name:'添加备用对话'}));fireEvent.click(await screen.findByRole('button',{name:'选择 备用 Codex'}));expect(screen.getByText('备用 1')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'上移接收对话 2'}));fireEvent.click(screen.getByRole('button',{name:'保存设置'}));await waitFor(()=>expect(api.save).toHaveBeenLastCalledWith({recoveryEnabled:true,alertTarget:{threadId:b.threadId,generation:b.generation,cwd:b.cwd},fallbackTargets:[{threadId:a.threadId,generation:a.generation,cwd:a.cwd}]}));
  await screen.findByText('已保存');fireEvent.click(screen.getByRole('button',{name:'移除接收对话 1'}));fireEvent.click(screen.getByRole('button',{name:'保存设置'}));await waitFor(()=>expect(api.save).toHaveBeenLastCalledWith({recoveryEnabled:true,alertTarget:{threadId:a.threadId,generation:a.generation,cwd:a.cwd},fallbackTargets:[]}));
 });
});

it('discovers current unwatched conversations and keeps joining separate from saving the receiver',async()=>{
 const old={threadId:'claude_code:old',generation:1,cwd:'D:/old',label:'旧的接收对话'};
 const current={threadId:'claude_code:new',generation:7,cwd:'D:/current',label:'当前管理对话'};
 const initial={settings:{recoveryEnabled:true,alertTarget:{threadId:old.threadId,generation:old.generation,cwd:old.cwd},fallbackTargets:[]},collectorAvailable:true,recent:[],targets:[old]};
 const api={read:vi.fn().mockResolvedValue(initial),save:vi.fn().mockImplementation(settings=>Promise.resolve({...initial,settings})),catalog:vi.fn().mockResolvedValue({complete:true,threads:[{id:current.threadId,label:current.label,projectProvenance:'D:/current',recencyAt:100},{id:'codex-other',label:'另一个项目',recencyAt:10}]}),select:vi.fn().mockResolvedValue(current)};
 render(<LocalSafetySettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'添加备用对话'}));
 expect(await screen.findByText('当前目录中不可用，请更换接收对话')).toBeVisible();
 fireEvent.change(screen.getByRole('searchbox',{name:'搜索对话或项目'}),{target:{value:'current'}});expect(screen.queryByRole('button',{name:'选择 另一个项目'})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'选择 当前管理对话'}));await waitFor(()=>expect(api.select).toHaveBeenCalledWith(current.threadId));expect(api.save).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'保存设置'}));await waitFor(()=>expect(api.save).toHaveBeenCalledWith({recoveryEnabled:true,alertTarget:{threadId:old.threadId,generation:1,cwd:old.cwd},fallbackTargets:[{threadId:current.threadId,generation:7,cwd:current.cwd}]}));
});
it('retains saved receivers when the current directory cannot be refreshed',async()=>{
 const old={threadId:'manager',generation:3,cwd:'D:/old',label:'已保存的管理'};const initial={settings:{recoveryEnabled:true,alertTarget:old},collectorAvailable:true,recent:[],targets:[old]};
 const api={read:vi.fn().mockResolvedValue(initial),save:vi.fn(),catalog:vi.fn().mockRejectedValue(Error('offline')),select:vi.fn()};
 render(<LocalSafetySettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'添加备用对话'}));expect(await screen.findByRole('alert')).toHaveTextContent('暂时读不到当前对话');expect(screen.getByText('已保存的管理')).toBeVisible();expect(api.save).not.toHaveBeenCalled();expect(api.select).not.toHaveBeenCalled();
});
