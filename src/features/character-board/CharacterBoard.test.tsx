import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {CharacterBoard,boardApi,type BoardSnapshot} from './CharacterBoard';
import {readModels} from '../workbench/readModelCache';
import {createHash} from 'node:crypto';
import {boardCache} from './boardCache';

const first:BoardSnapshot={revision:'one',html:'<html></html>',assets:[{id:'exact-image',source:'D:/approved/image.png'}]};
const hash=createHash('sha256').update('fixture').digest('hex');
const image={mime:'image/png',filename:'image.png',data:btoa('fixture'),assetId:'exact-image',contentHash:hash,sourceHash:hash};
beforeEach(()=>{readModels.clear();readModels.setScope(null);vi.spyOn(boardApi,'snapshot').mockResolvedValue(first);vi.spyOn(boardApi,'asset').mockResolvedValue(image);});
afterEach(()=>{cleanup();readModels.clear();readModels.dispose();vi.restoreAllMocks();});

it('keeps a sandboxed board mounted and does not re-read just because navigation returns',async()=>{
  const view=render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面');
  expect(frame).toHaveAttribute('sandbox','allow-scripts');expect(frame.getAttribute('src')).toBeNull();expect(frame.getAttribute('srcdoc')).toContain('Content-Security-Policy');
  view.rerender(<CharacterBoard active={false}/>);view.rerender(<CharacterBoard active/>);
  expect(screen.getByTitle('看板页面')).toBe(frame);expect(boardApi.snapshot).toHaveBeenCalledTimes(1);
  const previousUrl=frame.getAttribute('srcdoc');
  vi.mocked(boardApi.snapshot).mockResolvedValueOnce(first);
  fireEvent.click(screen.getByRole('button',{name:'刷新看板'}));
  await waitFor(()=>expect(boardApi.snapshot).toHaveBeenCalledTimes(2));await screen.findByText('已重新读取最新看板。');expect(screen.getByTitle('看板页面')).toBe(frame);expect(frame.getAttribute('srcdoc')).not.toBe(previousUrl);expect(boardApi.snapshot).toHaveBeenLastCalledWith(undefined,expect.any(AbortSignal));
});

it('only services the opaque child and its exact current catalog; duplicate requests cannot fan out',async()=>{
  render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面') as HTMLIFrameElement;await act(async()=>{});
  const channel=frame.dataset.boardChannel;
  const data={channel,kind:'asset',id:'exact-image',request:'1'};
  act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'https://wrong.invalid',source:frame.contentWindow,data})));
  act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:window,data})));
  act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data:{...data,id:'../secret'}})));
  expect(boardApi.asset).not.toHaveBeenCalled();
  act(()=>{for(let i=0;i<2;i++)window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data}));});
  await waitFor(()=>expect(boardApi.asset).toHaveBeenCalledTimes(1));expect(boardApi.asset).toHaveBeenCalledWith('one','exact-image','original',expect.any(AbortSignal));
});

it.each(['wrong identity','wrong bytes'])('refuses a cached response with %s instead of painting another character',async(problem)=>{
  vi.mocked(boardApi.asset).mockResolvedValueOnce(problem==='wrong identity'?{...image,assetId:'grey-reference'}:{...image,data:btoa('wrong picture')});
  render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面') as HTMLIFrameElement;await act(async()=>{});
  const post=vi.spyOn(frame.contentWindow!,'postMessage');const channel=frame.dataset.boardChannel;
  act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data:{channel,kind:'asset',id:'exact-image',request:'1',variant:'preview'}})));
  await waitFor(()=>expect(post).toHaveBeenCalledWith(expect.objectContaining({kind:'assetResult',request:'1',error:expect.any(String)}),'*'));
  expect(post.mock.calls.some(([m])=>m.kind==='assetResult'&&m.asset)).toBe(false);
});

it('reuses only verified media for the same stamped asset after an unrelated board revision',async()=>{
  render(<CharacterBoard active/>);let frame=await screen.findByTitle('看板页面') as HTMLIFrameElement;await act(async()=>{});
  const request=(id:string)=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data:{channel:frame.dataset.boardChannel,kind:'asset',id:'exact-image',request:id,variant:'preview'}}));
  const post=vi.spyOn(frame.contentWindow!,'postMessage');act(()=>{request('1');});
  await waitFor(()=>expect(post).toHaveBeenCalledWith(expect.objectContaining({kind:'assetResult',asset:image}),'*'));
  act(()=>readModels.put('board:character',{...first,revision:'two'}));
  await waitFor(()=>expect(frame.dataset.boardRevision).toBe('two'));
  const second=vi.spyOn(frame.contentWindow!,'postMessage');act(()=>{request('2');});
  await waitFor(()=>expect(second).toHaveBeenCalledWith(expect.objectContaining({kind:'assetResult',request:'2',asset:image}),'*'));
  expect(boardApi.asset).toHaveBeenCalledTimes(1);
});

it('retains the last successful page on a source read failure',async()=>{
  render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面');const url=frame.getAttribute('srcdoc');
  vi.mocked(boardApi.snapshot).mockRejectedValueOnce(new Error('BOARD_DATA_INCOMPLETE'));
  fireEvent.click(screen.getByRole('button',{name:'刷新看板'}));
  expect(await screen.findByText('暂时无法同步，稍后会自动重试。')).toBeInTheDocument();expect(frame.getAttribute('srcdoc')).toBe(url);
});

it('never grants a warm cached page approval authority before live revision confirmation',async()=>{
 readModels.put('board:character',first);vi.spyOn(boardApi,'review').mockResolvedValue({status:200,json:{}});
 render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面') as HTMLIFrameElement;await act(async()=>{});
 const post=vi.spyOn(frame.contentWindow!,'postMessage');
 act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data:{channel:frame.dataset.boardChannel,kind:'review',request:'1',input:{method:'POST',route:'/api/submit',body:{}}}})));
 expect(boardApi.review).not.toHaveBeenCalled();expect(post).toHaveBeenCalledWith(expect.objectContaining({kind:'reviewResult',response:expect.objectContaining({status:409})}),'*');
});

it('does not persist or paint an asset that finishes after the device cache was cleared',async()=>{
 let finish!:(value:typeof image)=>void;vi.mocked(boardApi.asset).mockImplementation(()=>new Promise(r=>{finish=r;}));const save=vi.spyOn(boardCache,'saveAsset');
 render(<CharacterBoard active/>);const frame=await screen.findByTitle('看板页面') as HTMLIFrameElement;await act(async()=>{});const post=vi.spyOn(frame.contentWindow!,'postMessage');
 act(()=>window.dispatchEvent(new MessageEvent('message',{origin:'null',source:frame.contentWindow,data:{channel:frame.dataset.boardChannel,kind:'asset',request:'1',id:'exact-image',variant:'preview'}})));
 await waitFor(()=>expect(boardApi.asset).toHaveBeenCalledOnce());await boardCache.clear();await act(async()=>{finish(image);});
 expect(save).not.toHaveBeenCalled();expect(post.mock.calls.some(([m])=>m.kind==='assetResult'&&m.asset)).toBe(false);
});
