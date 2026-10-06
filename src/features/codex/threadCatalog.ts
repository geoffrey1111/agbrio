import {t as uiText,getLanguage} from "../../i18n";
import type { ExistingCodexThreadCandidate } from "./types";
export type CatalogWindow = "1"|"3"|"7"|"30"|"ALL";
export type ThreadGroup = {key:string;label:string;threads:ExistingCodexThreadCandidate[]};
export function activityTime(thread:ExistingCodexThreadCandidate){return typeof thread.recencyAt==="number"&&Number.isFinite(thread.recencyAt)&&thread.recencyAt>=0?thread.recencyAt:null;}
export function projectKey(t:ExistingCodexThreadCandidate){return t.projectStatus==="PROJECT"&&t.projectId?`project:${t.projectId}`:t.projectStatus==="PROJECTLESS"?"projectless":"unconfirmed";}
export function projectLabel(t:ExistingCodexThreadCandidate){return projectKey(t)==="projectless"?uiText("无项目"):projectKey(t)==="unconfirmed"?uiText("项目归属未确认"):t.projectLabel||uiText("未命名项目");}
export function groupThreads(rows:ExistingCodexThreadCandidate[],range:CatalogWindow,query:string,project:string,nowSeconds:number):ThreadGroup[]{
 const cutoff=range==="ALL"?null:nowSeconds-Number(range)*86400;const q=query.trim().toLocaleLowerCase();const groups=new Map<string,ThreadGroup>();
 for(const row of rows){const at=activityTime(row),key=projectKey(row);if(cutoff!==null&&(at===null||at<cutoff))continue;if(project!=="ALL"&&project!==key)continue;if(q&&!`${row.label} ${row.id} ${projectLabel(row)}`.toLocaleLowerCase().includes(q))continue;const group=groups.get(key)??{key,label:projectLabel(row),threads:[]};group.threads.push(row);groups.set(key,group);}
 for(const group of groups.values())group.threads.sort((a,b)=>(activityTime(b)??-1)-(activityTime(a)??-1)||a.id.localeCompare(b.id));
 const result=[...groups.values()].sort((a,b)=>{const special=(g:ThreadGroup)=>g.key.startsWith("project:")?0:g.key==="projectless"?1:2;return special(a)-special(b)||(activityTime(b.threads[0])??-1)-(activityTime(a.threads[0])??-1)||a.key.localeCompare(b.key);});
 const counts=new Map<string,number>();for(const g of result)counts.set(g.label,(counts.get(g.label)??0)+1);for(const g of result)if((counts.get(g.label)??0)>1)g.label+=` · ${g.key.replace(/^project:/,"")}`;
 return result;
}
export function activityLabel(row:ExistingCodexThreadCandidate){const value=activityTime(row);return value===null?uiText("最近活动时间未确认"):new Intl.DateTimeFormat(getLanguage(),{dateStyle:"short",timeStyle:"short"}).format(new Date(value*1000));}
