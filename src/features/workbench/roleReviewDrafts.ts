import type { BridgeRole, RoleHandoff, RelayTextBlock } from "./RoleBridgePanel";
export type RoleReview = {
  role: BridgeRole; reply: {id:string;endpointId:string;text:string;observedAt?:number}; text:string;
  options:{id:string;filename:string;sha256?:string|null;size?:number|null}[];selected:string[];
  blocks?:RelayTextBlock[];blockIds?:string[];appliedBlockIds?:string[];choosing?:boolean;
  prepared?:RoleHandoff;approved?:RoleHandoff;bindingRevision?:number;destinationId?:string;
  /** Generated target-file references are separate from the owner's editable body. */
  attachmentManifest?:string;
};
const memory=new Map<string,Record<string,RoleReview>>();
const prefix="aiwr.role-review.v5.";
export function reviewKey(review:RoleReview){return `${review.role}:${review.reply.endpointId}:${review.reply.id}:${review.bindingRevision??"legacy"}`;}
export function readRoleReviews(workstream:string):Record<string,RoleReview>{
 if(memory.has(workstream))return memory.get(workstream)!;
 try{const v=JSON.parse(localStorage.getItem(prefix+workstream)??"{}");if(!v||Array.isArray(v)||typeof v!=="object")return{};
 return Object.fromEntries(Object.entries(v as Record<string,RoleReview>).filter(([key,r])=>r&&typeof r.text==="string"&&r.reply&&typeof r.reply.id==="string"&&typeof r.reply.endpointId==="string"&&typeof r.reply.text==="string"&&Array.isArray(r.options)&&Array.isArray(r.selected)&&["DECISION","EXECUTION"].includes(r.role)&&key===reviewKey(r)));
 }catch{return{};}
}
export function saveRoleReviews(workstream:string,reviews:Record<string,RoleReview>){memory.set(workstream,reviews);localStorage.setItem(prefix+workstream,JSON.stringify(reviews));}
export function clearRoleReviewCache(){memory.clear();for(let i=localStorage.length-1;i>=0;i--){const key=localStorage.key(i);if(key?.startsWith(prefix))localStorage.removeItem(key);}}
