import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {afterEach,describe,it,expect,vi} from 'vitest';
import {EventKeychainSettings,type EventKeychainApi} from './EventKeychainSettings';
afterEach(cleanup);
const cursor='agbrio-native-key:v1:89a21534-a51e-49e8-8a9b-41b06262f344';
function fixture():EventKeychainApi{return{status:vi.fn().mockResolvedValue({supported:true,storedReferences:65,pendingCleanup:1,needsOwnerAction:true}),authorize:vi.fn().mockResolvedValue({checked:1,nextCursor:null,notificationSent:false})};}
describe('owner-only Events Keychain access',()=>{
 it('has no authorization side effect on mount and carries only the server cursor between batches',async()=>{
  const api=fixture();vi.mocked(api.authorize).mockResolvedValueOnce({checked:64,nextCursor:cursor,notificationSent:false}).mockResolvedValueOnce({checked:1,nextCursor:null,notificationSent:false});render(<EventKeychainSettings api={api}/>);
  const button=await screen.findByRole('button',{name:'授权读取事件密钥'});expect(api.authorize).not.toHaveBeenCalled();fireEvent.click(button);
  fireEvent.click(await screen.findByRole('button',{name:'继续检查下一批'}));await waitFor(()=>expect(api.authorize).toHaveBeenCalledTimes(2));expect(vi.mocked(api.authorize).mock.calls).toEqual([[null],[cursor]]);
  expect(document.body.textContent).not.toContain(cursor);expect(document.querySelector('input[type=password]')).toBeNull();
 });
 it('shows native denial without automatically retrying',async()=>{
  const api=fixture();vi.mocked(api.authorize).mockRejectedValue('KEYCHAIN_OWNER_ACTION_REQUIRED');render(<EventKeychainSettings api={api}/>);fireEvent.click(await screen.findByRole('button',{name:'授权读取事件密钥'}));
  expect(await screen.findByRole('alert')).toHaveTextContent('macOS 需要本人授权或解锁');expect(api.authorize).toHaveBeenCalledExactlyOnceWith(null);
 });
 it('does not expose Mac controls on unsupported hosts',async()=>{
  const api=fixture();vi.mocked(api.status).mockResolvedValue({supported:false,storedReferences:0,pendingCleanup:0,needsOwnerAction:false});render(<EventKeychainSettings api={api}/>);await waitFor(()=>expect(api.status).toHaveBeenCalled());expect(screen.queryByRole('button')).not.toBeInTheDocument();expect(api.authorize).not.toHaveBeenCalled();
 });
 it('lets the owner unlock the store before the first key exists',async()=>{
  const api=fixture();vi.mocked(api.status).mockResolvedValue({supported:true,storedReferences:0,pendingCleanup:0,needsOwnerAction:false});vi.mocked(api.authorize).mockResolvedValue({checked:0,nextCursor:null,notificationSent:false});render(<EventKeychainSettings api={api}/>);
  const button=await screen.findByRole('button',{name:'授权读取事件密钥'});expect(button).not.toBeDisabled();expect(api.authorize).not.toHaveBeenCalled();fireEvent.click(button);await waitFor(()=>expect(api.authorize).toHaveBeenCalledExactlyOnceWith(null));
 });
});
