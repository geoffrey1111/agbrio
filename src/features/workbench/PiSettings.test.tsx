import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {describe,it,expect,vi,afterEach} from 'vitest';
import {PiSettings} from './PiSettings';
describe('Pi owner permissions',()=>{
 afterEach(cleanup);
 it('grants an explicit assistant connection with machine workspace, write, terminal and context defaults',async()=>{
  const initial={installed:true,version:'1.4.0',scopes:[],assistantGrants:[{id:'allowed-connection',label:'管理助手'}]};
  const api={read:vi.fn().mockResolvedValue(initial),authorize:vi.fn(),revoke:vi.fn(),authorizeWorkspace:vi.fn().mockResolvedValue(initial)};
  render(<PiSettings api={api}/>);const submit=await screen.findByRole('button',{name:'授权工作范围'});await waitFor(()=>expect(submit).toBeEnabled());
  fireEvent.click(submit);await waitFor(()=>expect(api.authorizeWorkspace).toHaveBeenCalledWith({roots:[],machineScope:true,writable:true,terminal:true,administrator:true,inheritContext:true,grantIds:['allowed-connection']}));
  fireEvent.click(screen.getByRole('checkbox',{name:'允许读写项目文件'}));expect(screen.getByRole('checkbox',{name:'允许执行任务所需的终端命令'})).not.toBeChecked();expect(screen.getByRole('checkbox',{name:'允许执行任务所需的终端命令'})).toBeDisabled();
 });
 it('prepares administrator execution only after an explicit desktop action',async()=>{
  const initial={installed:true,version:'1.4.0',scopes:[],administratorBackend:{ready:false}};
  const api={read:vi.fn().mockResolvedValue(initial),authorize:vi.fn(),revoke:vi.fn(),prepareAdministrator:vi.fn().mockResolvedValue({...initial,administratorBackend:{ready:true}})};
  render(<PiSettings api={api}/>);const prepare=await screen.findByRole('button',{name:'准备管理员后台'});expect(api.prepareAdministrator).not.toHaveBeenCalled();fireEvent.click(prepare);
  await screen.findByText('管理员后台已就绪，新 Pi 子任务会直接复用。');expect(api.prepareAdministrator).toHaveBeenCalledTimes(1);expect(screen.queryByRole('button',{name:'准备管理员后台'})).not.toBeInTheDocument();
 });
 it('shows every authorized drive instead of only the legacy primary anchor',async()=>{
  const policy={roots:['C:\\','D:\\','E:\\'],machineScope:true,writable:true,terminal:true,administrator:true,inheritContext:true,grantIds:['manager']};
  const api={read:vi.fn().mockResolvedValue({installed:true,version:'1.4.0',scopes:[{id:'machine',root:'C:\\',generation:1,writable:true,enabled:true,policy}]}),authorize:vi.fn(),revoke:vi.fn()};
  render(<PiSettings api={api}/>);const drives=await screen.findByText(/已授权磁盘/);for(const drive of policy.roots)expect(drives).toHaveTextContent(drive);
 });
 it('defaults to read-only and revokes using the exact scope generation',async()=>{
  const scope={id:'scope-exact',root:'D:\Projects\Example',generation:4,writable:false,enabled:true};const initial={installed:true,version:'1.4.0',scopes:[]};const api={read:vi.fn().mockResolvedValue(initial),authorize:vi.fn().mockResolvedValue({...initial,scopes:[scope]}),revoke:vi.fn().mockResolvedValue(initial)};
  render(<PiSettings api={api}/>);await screen.findByRole('textbox',{name:'项目文件夹'});expect(screen.getByRole('checkbox')).not.toBeChecked();fireEvent.change(screen.getByRole('textbox'),{target:{value:scope.root}});fireEvent.click(screen.getByRole('button',{name:'授权目录'}));await screen.findByText(scope.root);expect(api.authorize).toHaveBeenCalledWith(scope.root,false);fireEvent.click(screen.getByRole('button',{name:'撤销并停止'}));await waitFor(()=>expect(api.revoke).toHaveBeenCalledWith(scope.id,4));
 });
 it('explains a blocked directory without discarding the chosen path',async()=>{
  const api={read:vi.fn().mockResolvedValue({installed:true,version:'1.4.0',scopes:[]}),authorize:vi.fn().mockRejectedValue('PI_SCOPE_PROTECTED_DIRECTORY'),revoke:vi.fn()};render(<PiSettings api={api}/>);const input=await screen.findByRole('textbox',{name:'项目文件夹'});fireEvent.change(input,{target:{value:'C:\Users\Owner'}});fireEvent.click(screen.getByRole('button',{name:'授权目录'}));expect(await screen.findByRole('alert')).toHaveTextContent('具体项目文件夹');expect(input).toHaveValue('C:\Users\Owner');
 });
});
