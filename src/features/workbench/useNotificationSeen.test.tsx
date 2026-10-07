import {useRef,useState} from 'react';
import {render,cleanup,act} from '@testing-library/react';
import {afterEach,it,expect,vi} from 'vitest';
import {useNotificationSeen} from './useNotificationSeen';
let callback:IntersectionObserverCallback;
const unobserve=vi.fn(),disconnect=vi.fn();
function observers(){vi.stubGlobal('IntersectionObserver',class{constructor(cb:IntersectionObserverCallback){callback=cb;}observe(){}unobserve=unobserve;disconnect=disconnect;});}
function show(node:Element,ratio:number){act(()=>callback([{target:node,isIntersecting:ratio>0,intersectionRatio:ratio} as IntersectionObserverEntry],{} as IntersectionObserver));}
function Harness({enabled=true,mark,onSeen}:{enabled?:boolean;mark:(n:number)=>Promise<unknown>;onSeen:(n:number)=>void}){const ref=useRef<HTMLDivElement>(null);useNotificationSeen(ref,enabled,[1,2],[1,2],mark,onSeen,()=>{});return <div ref={ref}><article data-notification-sequence="1"/><article data-notification-sequence="2"/></div>;}
afterEach(()=>{cleanup();vi.useRealTimers();vi.unstubAllGlobals();vi.clearAllMocks();});
it('marks only a card half visible for the dwell interval and leaves scrolling slivers unread',async()=>{
 vi.useFakeTimers();observers();const mark=vi.fn(async()=>({})),seen=vi.fn();const view=render(<Harness mark={mark} onSeen={seen}/>);const nodes=view.container.querySelectorAll('article');
 show(nodes[0],.1);show(nodes[1],.8);await act(()=>vi.advanceTimersByTimeAsync(599));expect(mark).not.toHaveBeenCalled();
 await act(()=>vi.advanceTimersByTimeAsync(1));expect(mark).toHaveBeenCalledExactlyOnceWith(2);expect(seen).toHaveBeenCalledExactlyOnceWith(2);expect(unobserve).toHaveBeenCalledWith(nodes[1]);
});
it('leaving the card or navigating away cancels pending read acknowledgement',async()=>{
 vi.useFakeTimers();observers();const mark=vi.fn(async()=>({})),seen=vi.fn();const view=render(<Harness mark={mark} onSeen={seen}/>);const node=view.container.querySelector('article')!;
 show(node,.8);await act(()=>vi.advanceTimersByTimeAsync(300));show(node,0);await act(()=>vi.advanceTimersByTimeAsync(1000));expect(mark).not.toHaveBeenCalled();
 show(node,.8);view.rerender(<Harness enabled={false} mark={mark} onSeen={seen}/>);await act(()=>vi.advanceTimersByTimeAsync(1000));expect(mark).not.toHaveBeenCalled();
});

it('concurrent read responses both update the badge despite a rerender after the first',async()=>{
 vi.useFakeTimers();observers();const resolve=new Map<number,()=>void>();const mark=vi.fn((n:number)=>new Promise<void>(r=>resolve.set(n,r)));const seen=vi.fn();
 function Stateful(){const ref=useRef<HTMLDivElement>(null),[unread,setUnread]=useState([1,2]);useNotificationSeen(ref,true,[1,2],unread,mark,n=>{seen(n);setUnread(old=>old.filter(v=>v!==n));},()=>{});return <div ref={ref}><article data-notification-sequence="1"/><article data-notification-sequence="2"/><span>{unread.length}</span></div>;}
 const view=render(<Stateful/>);const nodes=view.container.querySelectorAll('article');show(nodes[0],.8);show(nodes[1],.8);await act(()=>vi.advanceTimersByTimeAsync(600));
 await act(async()=>resolve.get(1)?.());await act(async()=>resolve.get(2)?.());expect(seen.mock.calls.map(v=>v[0])).toEqual([1,2]);expect(view.container.querySelector('span')).toHaveTextContent('0');
});
