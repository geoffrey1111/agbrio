import {afterEach,expect,it,vi} from 'vitest';
import {installStandaloneTopEdge} from './standaloneTopEdge';
afterEach(()=>{document.getElementById('aiwr-standalone-top-edge')?.remove();vi.unstubAllGlobals();});
function platform(userAgent:string,standalone:boolean,supported=true){
 vi.stubGlobal('navigator',{userAgent,standalone,platform:'iPhone',maxTouchPoints:5});
 vi.stubGlobal('CSS',{supports:()=>supported});
}
it('installs one noninteractive body sibling only for installed Apple mobile pages',()=>{
 platform('iPhone',true);installStandaloneTopEdge();installStandaloneTopEdge();
 const edges=document.querySelectorAll('#aiwr-standalone-top-edge');expect(edges).toHaveLength(1);
 expect(edges[0].parentElement).toBe(document.body);expect(edges[0].getAttribute('aria-hidden')).toBe('true');
 expect(edges[0].childNodes).toHaveLength(0);expect(edges[0].hasAttribute('tabindex')).toBe(false);
});
it('leaves desktop and normal Safari tabs untouched',()=>{
 platform('Windows',true);installStandaloneTopEdge();expect(document.getElementById('aiwr-standalone-top-edge')).toBeNull();
 platform('iPhone',false);installStandaloneTopEdge();expect(document.getElementById('aiwr-standalone-top-edge')).toBeNull();
});
it('fails closed without text-clipping support instead of covering controls',()=>{
 platform('iPhone',true,false);installStandaloneTopEdge();expect(document.getElementById('aiwr-standalone-top-edge')).toBeNull();
});
