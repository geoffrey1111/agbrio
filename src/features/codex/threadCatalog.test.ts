import {describe,it,expect} from "vitest";
import {groupThreads,activityTime} from "./threadCatalog";
import type {ExistingCodexThreadCandidate} from "./types";
const now=1790985600;
const row=(id:string,days:number,projectId:string|null="a"):ExistingCodexThreadCandidate=>({id,label:"同名对话",recencyAt:now-days*86400,projectId,projectLabel:"同名项目",projectStatus:projectId?"PROJECT":"PROJECTLESS"});
describe("native recency and exact Desktop grouping",()=>{
 it("uses rolling 1/3/7/30/all windows and never creation/update/scan time",()=>{const rows=[row("one",.5),row("three",2),row("seven",5),row("month",20),row("older",40),{...row("unknown",0),recencyAt:null,updatedAt:String(now)}];const counts=["1","3","7","30","ALL"].map(w=>groupThreads(rows,w as "1", "","ALL",now).flatMap(g=>g.threads).length);expect(counts).toEqual([1,2,3,4,6]);expect(activityTime(rows[5])).toBeNull();});
 it("separates duplicate project labels and unknown membership; sorts activity",()=>{const rows=[row("old",.8,"a"),row("new",.1,"a"),row("other",.3,"b"),row("free",.2,null),{...row("missing",.2),projectStatus:"UNCONFIRMED" as const,projectId:null}];const groups=groupThreads(rows,"ALL","","ALL",now);expect(groups.map(g=>g.key)).toEqual(["project:a","project:b","projectless","unconfirmed"]);expect(groups[0].threads.map(t=>t.id)).toEqual(["new","old"]);expect(groups[0].label).not.toBe(groups[1].label);expect(groupThreads(rows,"ALL","other","project:b",now)[0].threads[0].id).toBe("other");});
});
