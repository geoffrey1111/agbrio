import {describe,it,expect} from "vitest";
import {visibleViewportInsets,resolveVisibleFrame} from "./mobileViewport";

const layout={width:390,height:844};
const safe={top:59,right:0,bottom:34,left:0};
describe("visible phone safe areas",()=>{
 it("reserves the real notch and home gesture once in a full viewport",()=>{
  expect(visibleViewportInsets(layout,{top:0,left:0,...layout},safe)).toEqual({bottom:0,safe});
 });
 it("anchors sheets above the keyboard and drops the already hidden home inset",()=>{
  expect(visibleViewportInsets(layout,{top:0,left:0,width:390,height:480},safe)).toEqual({bottom:364,safe:{...safe,bottom:0}});
 });
 it("subtracts only covered safe area during keyboard focus panning",()=>{
  expect(visibleViewportInsets(layout,{top:40,left:0,width:390,height:440},safe)).toEqual({bottom:364,safe:{...safe,top:19,bottom:0}});
 });
 it("supports horizontal safe areas and returns them after rotation or keyboard dismissal",()=>{
  expect(visibleViewportInsets({width:844,height:390},{top:0,left:59,width:785,height:390},{top:0,right:59,bottom:21,left:59})).toEqual({bottom:0,safe:{top:0,right:59,bottom:21,left:0}});
  expect(visibleViewportInsets(layout,{top:0,left:0,...layout},safe).safe).toEqual(safe);
 });
});

describe("Home Screen viewport measurements",()=>{
 const frame={top:0,left:0,...layout};
 it("recovers CSS display space when WebKit already deducted notch and gesture areas",()=>{
  expect(resolveVisibleFrame(frame,{...frame,height:751},safe,true)).toEqual(frame);
 });
 it("keeps an actual keyboard viewport and focus pan",()=>{
  const keyboard={...frame,height:440,top:24};
  expect(resolveVisibleFrame(frame,keyboard,safe,true)).toEqual(keyboard);
 });
 it("does not inflate a browser viewport or an OS-owned opaque status strip",()=>{
  const browser={...frame,height:751};
  expect(resolveVisibleFrame(frame,browser,safe,false)).toEqual(browser);
  expect(resolveVisibleFrame(frame,browser,{...safe,top:0,bottom:0},true)).toEqual(browser);
 });
 it("does not invent the owner's missing62px outside its measured CSS frame",()=>{
  const actual={top:0,left:0,width:440,height:894};
  expect(resolveVisibleFrame(actual,actual,{top:0,right:0,bottom:34,left:0},true)).toEqual(actual);
  expect(visibleViewportInsets(actual,actual,{top:0,right:0,bottom:34,left:0}).safe.bottom).toBe(34);
 });
});
