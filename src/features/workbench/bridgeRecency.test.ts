import {describe,it,expect} from "vitest";
import {latestBridgeReply,replyTime} from "./bridgeRecency";
import type {RoleState} from "./RoleBridgePanel";
const state:RoleState={bindings:{workstreamId:"qa",bindingRevision:1,explicitRoles:true,decision:{role:"DECISION",endpoint:{id:"a",externalId:"ta",provider:"CODEX",label:"A"}},execution:{role:"EXECUTION",endpoint:{id:"b",externalId:"tb",provider:"CODEX",label:"B"}}},handoffs:[],replies:[{id:"ra",endpointId:"a",text:"A",observedAt:5000,completedAt:1000},{id:"rb",endpointId:"b",text:"B",observedAt:3000,completedAt:2000}]};
describe("Bridge reply recency",()=>{
 it("uses completion chronology despite reversed observation order",()=>expect(latestBridgeReply(state)).toMatchObject({role:"EXECUTION",byCompletion:true}));
 it("cannot select a reply belonging to a previous binding",()=>expect(latestBridgeReply({...state,replies:[...state.replies,{id:"orphan",endpointId:"old",text:"old",observedAt:9999,completedAt:9999}]})).toMatchObject({role:"EXECUTION"}));
 it("reports receipt fallback when completion data is missing",()=>{const old={...state,replies:state.replies.map(r=>({...r,completedAt:null}))};expect(latestBridgeReply(old)).toMatchObject({role:"DECISION",byCompletion:false});expect(replyTime(old.replies[0])).toEqual({at:5000,completed:false});});
});
