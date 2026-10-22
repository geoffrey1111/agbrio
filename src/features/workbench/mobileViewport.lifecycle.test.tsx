import {act,cleanup,renderHook} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {useVisibleViewport} from './mobileViewport';

afterEach(()=>{cleanup();vi.useRealTimers();vi.unstubAllGlobals();document.documentElement.removeAttribute('style');});
function fixture(){
 const viewport=Object.assign(new EventTarget(),{width:390,height:440,offsetTop:24,offsetLeft:0,scale:1});
 vi.stubGlobal('visualViewport',viewport);vi.stubGlobal('innerWidth',390);vi.stubGlobal('innerHeight',844);
 renderHook(()=>useVisibleViewport());return viewport;
}
it('drops keyboard geometry during zoom and recovers the full frame afterwards',()=>{
 const viewport=fixture(),style=document.documentElement.style;
 expect(style.getPropertyValue('--aiwr-shell-height')).toBe('440px');
 act(()=>{viewport.scale=1.5;viewport.height=300;viewport.dispatchEvent(new Event('resize'));});
 expect(style.getPropertyValue('--aiwr-shell-height')).toBe('');expect(style.getPropertyValue('--aiwr-shell-top')).toBe('');
 act(()=>{viewport.scale=1;viewport.height=844;viewport.offsetTop=0;viewport.dispatchEvent(new Event('resize'));});
 expect(style.getPropertyValue('--aiwr-shell-height')).toBe('844px');expect(style.getPropertyValue('--aiwr-shell-top')).toBe('0px');
});
it('resamples delayed keyboard dismissal and restored pages without a viewport resize',()=>{
 vi.useFakeTimers();const viewport=fixture(),style=document.documentElement.style;
 act(()=>document.dispatchEvent(new FocusEvent('focusout')));
 act(()=>{viewport.height=844;viewport.offsetTop=0;vi.advanceTimersByTime(350);});
 expect(style.getPropertyValue('--aiwr-shell-height')).toBe('844px');
 act(()=>{viewport.height=440;viewport.offsetTop=24;viewport.dispatchEvent(new Event('resize'));window.dispatchEvent(new Event('pageshow'));});
 act(()=>{viewport.height=844;viewport.offsetTop=0;vi.advanceTimersByTime(350);});
 expect(style.getPropertyValue('--aiwr-shell-height')).toBe('844px');
 cleanup();expect(vi.getTimerCount()).toBe(0);
});
