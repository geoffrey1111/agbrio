import {bridgeReviewAttention} from "./bridgeActivity";
import {cleanup,render,screen} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {BridgeActivityStrip} from "./BridgeActivityStrip";
import {setLanguagePreference} from "../../i18n";
import type {BridgeActivity} from "./bridgeActivity";
vi.mock("gsap",()=>({gsap:{matchMedia:()=>({add:vi.fn(),revert:vi.fn()})}}));
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
const activity:BridgeActivity={workstreamId:"exact",bindingRevision:2,sides:[{role:"DECISION",endpointId:"d",state:"THINKING",checkedAt:100},{role:"EXECUTION",endpointId:"e",state:"ACTION_REQUIRED",checkedAt:100}]};
it("shows only the active side, and removes stale busy feedback",()=>{
 const {rerender}=render(<BridgeActivityStrip activity={activity} age={0}/>);expect(screen.getByText("正在思考")).toBeVisible();expect(screen.queryByText("执行")).toBeNull();expect(screen.getAllByText("控制")).toHaveLength(1);
 rerender(<BridgeActivityStrip activity={activity} age={15000}/>);expect(screen.queryByText("正在思考")).toBeNull();
});
it("retains native stalled Goal meaning independently of older saved results",()=>{
 render(<BridgeActivityStrip activity={{...activity,sides:[{...activity.sides[1],goalStatus:"blocked"}]}} age={0}/>);expect(screen.getByText("目标已停滞")).toBeVisible();expect(screen.queryByText("最新回复")).toBeNull();
});
it("reports concurrent execution honestly instead of arbitrarily choosing one active side",()=>{
 render(<BridgeActivityStrip activity={{...activity,sides:[activity.sides[0],{...activity.sides[1],state:"RUNNING"}]}} age={0}/>);expect(screen.getByText("两端同时执行")).toBeVisible();expect(document.querySelectorAll('.r2-agent-state')).toHaveLength(1);
});

it("does not count an active but idle Goal as a second executing conversation",()=>{
 render(<BridgeActivityStrip activity={{...activity,sides:[{...activity.sides[0],state:"RUNNING",turnActive:false,goalStatus:"active"},{...activity.sides[1],state:"RUNNING",turnActive:true}]}} age={0}/>);expect(screen.getByText("执行")).toBeVisible();expect(screen.getByText("正在执行")).toBeVisible();expect(screen.queryByText("两端同时执行")).toBeNull();
});

it("review red dots appear only after stopping for owner attention, never while execution continues",()=>{
 const running={...activity,unreadCount:3};expect(bridgeReviewAttention(running,0)).toBe(0);
 expect(bridgeReviewAttention({...activity,unreadCount:0,sides:[{...activity.sides[1],goalStatus:"blocked"}]},0)).toBe(1);
 expect(bridgeReviewAttention({...activity,unreadCount:2,sides:[{...activity.sides[1],state:"COMPLETE"}]},0)).toBe(2);
 expect(bridgeReviewAttention({...activity,unreadCount:2,sides:[{...activity.sides[1],state:"COMPLETE"}]},15000)).toBe(0);
});

it("an active Goal wins over another side's older completed result without inventing an active turn",()=>{
 render(<BridgeActivityStrip activity={{...activity,latestRole:"DECISION",sides:[{...activity.sides[0],state:"COMPLETE"},{...activity.sides[1],state:"RUNNING",turnActive:false,goalStatus:"active"}]}} age={0}/>);expect(screen.queryByText("最新回复")).toBeNull();expect(screen.getByText("执行")).toBeVisible();expect(screen.queryByText("两端同时执行")).toBeNull();
});
