/** DiceBear Sprouts and Critters (CC0 1.0), generated once into public/avatars.
 * Each conversation keeps the avatar it was first given; new conversations take
 * an unused one until all 72 are in use, then repeats are allowed. */
import {useMemo} from "react";
export const AVATARS=["sprouts-01","critters-01","sprouts-02","critters-02","sprouts-03","critters-03","sprouts-04","critters-04","sprouts-05","critters-05","sprouts-06","critters-06","sprouts-07","critters-07","sprouts-08","critters-08","sprouts-09","critters-09","sprouts-10","critters-10","sprouts-11","critters-11","sprouts-12","critters-12","sprouts-13","critters-13","sprouts-14","critters-14","sprouts-15","critters-15","sprouts-16","critters-16","sprouts-17","critters-17","sprouts-18","critters-18","sprouts-19","critters-19","sprouts-20","critters-20","sprouts-21","critters-21","sprouts-22","critters-22","sprouts-23","critters-23","sprouts-24","critters-24","sprouts-25","critters-25","sprouts-26","critters-26","sprouts-27","critters-27","sprouts-28","critters-28","sprouts-29","critters-29","sprouts-30","critters-30","sprouts-31","critters-31","sprouts-32","critters-32","sprouts-33","critters-33","sprouts-34","critters-34","sprouts-35","critters-35","sprouts-36","critters-36"] as const;
const STORE="agbrio.avatars.v1";
const hash=(s:string)=>{let h=2166136261;for(let i=0;i<s.length;i++){h^=s.charCodeAt(i);h=Math.imul(h,16777619);}return h>>>0;};
let memory:Record<string,number>|null=null;
function load(){if(memory)return memory;try{const v=JSON.parse(localStorage.getItem(STORE)??"{}");memory=v&&typeof v==="object"?v:{};}catch{memory={};}return memory!;}
function save(){try{localStorage.setItem(STORE,JSON.stringify(memory));}catch{/* per-device preference only */}}
export function avatarFor(id:string){
 const map=load();
 if(!(id in map)){
  const used=new Set(Object.values(map));const start=hash(id)%AVATARS.length;let pick=start;
  if(used.size<AVATARS.length)for(let step=0;step<AVATARS.length;step++){const at=(start+step)%AVATARS.length;if(!used.has(at)){pick=at;break;}}
  map[id]=pick;save();
 }
 return `/avatars/${AVATARS[map[id]]}.svg`;
}
export function useAvatars(ids:string[]){const key=ids.join("|");return useMemo(()=>Object.fromEntries(ids.map(id=>[id,avatarFor(id)])),[key]);}
export function resetAvatarMemoryForTests(){memory=null;}
