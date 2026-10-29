import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {describe,it,expect,vi,afterEach} from 'vitest';
import {LocalSafetySettings,localSafetyApi} from './LocalSafetySettings';
describe('owner safety settings',()=>{
 afterEach(cleanup);
 it('defaults recovery to all watched Codex threads and leaves the manager unbound until saved',async()=>{
  const initial={settings:{recoveryEnabled:true,alertTarget:null},collectorAvailable:true,recent:[],targets:[{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager',label:'管理对话'}]};
  const api={read:vi.fn().mockResolvedValue(initial),watches:vi.fn().mockResolvedValue([{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager',enabled:true,label:'管理对话'}]),save:vi.fn().mockImplementation(settings=>Promise.resolve({...initial,settings}))} as unknown as typeof localSafetyApi;
  render(<LocalSafetySettings api={api}/>);
  expect(await screen.findByLabelText('自动恢复 Codex 模型故障')).toBeChecked();expect(screen.getByLabelText('资源告警接收对话')).toHaveValue('');expect(api.save).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText('资源告警接收对话'),{target:{value:'claude_code:exact'}});fireEvent.click(screen.getByRole('button',{name:'保存设置'}));
  await waitFor(()=>expect(api.save).toHaveBeenCalledWith({recoveryEnabled:true,alertTarget:{threadId:'claude_code:exact',generation:3,cwd:'D:\\manager'},fallbackTargets:[]}));expect(await screen.findByText('已保存')).toBeInTheDocument();
 });
 it('shows failed read without implying protection is active',async()=>{const api={read:vi.fn().mockRejectedValue(Error('offline')),watches:vi.fn().mockResolvedValue([])} as unknown as typeof localSafetyApi;render(<LocalSafetySettings api={api}/>);expect(await screen.findByRole('alert')).toHaveTextContent('暂时无法读取保护设置');expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();});
 it('adds ordered backups, promotes one with arrows and removes without losing the remaining bindings',async()=>{
  const a={threadId:'claude_code:primary',generation:1,cwd:'D:\\primary',label:'主 Claude'},b={threadId:'codex-backup',generation:2,cwd:'D:\\backup',label:'备用 Codex'};
  const initial={settings:{recoveryEnabled:true,alertTarget:null,fallbackTargets:[]},collectorAvailable:true,recent:[],targets:[a,b]};const api={read:vi.fn().mockResolvedValue(initial),save:vi.fn().mockImplementation(settings=>Promise.resolve({...initial,settings}))} as typeof localSafetyApi;
  render(<LocalSafetySettings api={api}/>);const select=await screen.findByLabelText('资源告警接收对话');fireEvent.change(select,{target:{value:a.threadId}});fireEvent.change(select,{target:{value:b.threadId}});expect(screen.getByText('备用 1')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'上移接收对话 2'}));fireEvent.click(screen.getByRole('button',{name:'保存设置'}));await waitFor(()=>expect(api.save).toHaveBeenLastCalledWith({recoveryEnabled:true,alertTarget:{threadId:b.threadId,generation:b.generation,cwd:b.cwd},fallbackTargets:[{threadId:a.threadId,generation:a.generation,cwd:a.cwd}]}));
  await screen.findByText('已保存');fireEvent.click(screen.getByRole('button',{name:'移除接收对话 1'}));fireEvent.click(screen.getByRole('button',{name:'保存设置'}));await waitFor(()=>expect(api.save).toHaveBeenLastCalledWith({recoveryEnabled:true,alertTarget:{threadId:a.threadId,generation:a.generation,cwd:a.cwd},fallbackTargets:[]}));
 });
});
