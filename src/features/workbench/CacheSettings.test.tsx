import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {liveRequest} from '../../mobile/api';
import {boardCache} from '../character-board/boardCache';
import {CacheSettings} from './CacheSettings';
vi.mock('@tauri-apps/api/core',()=>({isTauri:vi.fn(()=>false),invoke:vi.fn()}));
vi.mock('../../mobile/api',()=>({liveRequest:vi.fn()}));
vi.mock('../character-board/boardCache',()=>({BOARD_CACHE_LIMIT:80*1024*1024,boardCache:{stats:vi.fn(async()=>({bytes:1048576,items:1})),clearPresentation:vi.fn(async()=>{}),pruneExpired:vi.fn(),setScope:vi.fn()}}));
afterEach(()=>{cleanup();vi.clearAllMocks();vi.mocked(isTauri).mockReturnValue(false);});
const report={bytes:4194304,eligibleBytes:1048576,protectedBytes:3145728,uploadCount:3,eligibleCount:1,protectedCount:2,removedCount:0,retentionDays:7};
it('phone shows storage, clears only presentation and eligible uploads through owner API',async()=>{
 vi.mocked(liveRequest).mockImplementation(async(path)=>({...report,removedCount:path.endsWith('/cleanup')?1:0}) as any);
 render(<CacheSettings/>);fireEvent.click(screen.getByText('存储与缓存'));await screen.findByText('电脑临时附件 · 4.0 MB');
 expect(screen.getByText('对话附件（保留） · 3.0 MB')).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'清理缓存'}));
 await screen.findByText('已清理缓存：1 个临时附件。');expect(boardCache.clearPresentation).toHaveBeenCalledOnce();
 expect(liveRequest).toHaveBeenCalledWith('/cache/cleanup',{method:'POST',body:'{}'});expect(invoke).not.toHaveBeenCalled();
});
it('desktop uses native maintenance and a failed computer cleanup still clears the local preview',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockImplementation(async(command)=>{if(command==='app_cache_cleanup')throw Error('CACHE_BUSY');return report as any;});
 render(<CacheSettings/>);fireEvent.click(screen.getByText('存储与缓存'));await screen.findByText('电脑临时附件 · 4.0 MB');fireEvent.click(screen.getByRole('button',{name:'清理缓存'}));
 await screen.findByText('此设备缓存已清理，电脑端暂时无法清理。');await waitFor(()=>expect(screen.getByRole('button',{name:'清理缓存'})).toBeEnabled());expect(invoke).toHaveBeenCalledWith('app_cache_cleanup');expect(liveRequest).not.toHaveBeenCalled();
});
