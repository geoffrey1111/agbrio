import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {CodexQuota,type CodexQuotaSnapshot} from './CodexQuota';
import {setLanguagePreference} from '../../i18n';
const native:CodexQuotaSnapshot={status:'AVAILABLE',reason:null,observedAt:1791540000000,buckets:[{id:'codex',windows:[{slot:'primary',usedPercent:36,remainingPercent:64,windowDurationMins:10080,resetsAt:1791543600}]}]};
afterEach(()=>{cleanup();setLanguagePreference('zh-CN');});
it('shows actual weekly remaining and converts native reset seconds, without inventing a secondary window',async()=>{
 const read=vi.fn().mockResolvedValue(native);render(<CodexQuota api={{read}}/>);expect(await screen.findByText('每周剩余 64%')).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'查看 Codex 额度'}));expect(screen.getByRole('meter')).toHaveAttribute('aria-valuenow','64');expect(screen.queryByText('5 小时')).toBeNull();expect(screen.getByText(/重置于/).textContent).not.toContain('1970');expect(read).toHaveBeenCalledOnce();
});
it('deduplicates refresh while pending and keeps zero distinct from unavailable',async()=>{
 let resolve!:(data:CodexQuotaSnapshot)=>void;const read=vi.fn().mockImplementation(()=>new Promise(r=>{resolve=r;}));render(<CodexQuota api={{read}}/>);fireEvent.click(screen.getByRole('button',{name:'查看 Codex 额度'}));expect(screen.getByRole('button',{name:'读取中…'})).toBeDisabled();expect(read).toHaveBeenCalledOnce();
 await act(async()=>resolve({...native,buckets:[{id:'codex',windows:[{...native.buckets[0].windows[0],remainingPercent:0,usedPercent:100}]}]}));expect(screen.getByRole('meter')).toHaveAttribute('aria-valuenow','0');
});
it('labels a failed refresh as last read, but clears data if the account is changed or unsupported',async()=>{
 const read=vi.fn().mockResolvedValueOnce(native).mockRejectedValueOnce(Error('offline')).mockResolvedValue({status:'UNAVAILABLE',reason:'ACCOUNT_CHANGED',observedAt:1,buckets:[]});render(<CodexQuota api={{read}}/>);await screen.findByText('每周剩余 64%');fireEvent.click(screen.getByRole('button',{name:'查看 Codex 额度'}));fireEvent.click(screen.getByRole('button',{name:'刷新额度'}));await screen.findByText('连接暂不可用，以下为上次读取结果。');fireEvent.click(screen.getByRole('button',{name:'刷新额度'}));await waitFor(()=>expect(screen.queryByRole('meter')).toBeNull());expect(screen.queryByText(/64%/)).toBeNull();
});
it.each(['en','zh-TW'] as const)('uses consistent %s chrome for multi-window native data',async language=>{
 setLanguagePreference(language);render(<CodexQuota api={{read:async()=>({...native,buckets:[{id:'codex',windows:[...native.buckets[0].windows,{slot:'secondary',usedPercent:0,remainingPercent:100,windowDurationMins:300,resetsAt:null}]}]})}}/>);fireEvent.click(screen.getByRole('button',{name:language==='en'?'View Codex usage':'查看 Codex 額度'}));await waitFor(()=>expect(screen.getAllByRole('meter')).toHaveLength(2));if(language==='en')expect(screen.getByRole('dialog').textContent).not.toMatch(/[\u3400-\u9fff]/);
});
it('ignores a late response from a replaced API instance',async()=>{
 let resolve!:(d:CodexQuotaSnapshot)=>void;const first={read:()=>new Promise<CodexQuotaSnapshot>(r=>{resolve=r;})};const {rerender}=render(<CodexQuota api={first}/>);rerender(<CodexQuota api={{read:async()=>({status:'UNAVAILABLE',reason:'ACCOUNT_UNSUPPORTED',observedAt:1,buckets:[]})}}/>);await screen.findByText('暂不可读取');await act(async()=>resolve(native));expect(screen.queryByText(/64%/)).toBeNull();
});
it('closes a hidden quota sheet while retaining its last reading and refresh clock',async()=>{
 const api={read:vi.fn().mockResolvedValue(native)};const{rerender}=render(<CodexQuota api={api} active/>);await screen.findByText('每周剩余 64%');fireEvent.click(screen.getByRole('button',{name:'查看 Codex 额度'}));expect(screen.getByRole('dialog')).toBeVisible();rerender(<CodexQuota api={api} active={false}/>);expect(screen.queryByRole('dialog')).toBeNull();rerender(<CodexQuota api={api} active/>);expect(screen.getByText('每周剩余 64%')).toBeVisible();expect(api.read).toHaveBeenCalledTimes(1);
});
