import type {CodexWatch} from './CodexNotifications';

const root=(value:string)=>value.replace(/^\\\\\?\\/,'').replaceAll('\\','/').replace(/\/+$/,'').toLocaleLowerCase();
/** Presentation only: identities come from the server's reviewed association. */
export function piDelegations(watches:CodexWatch[]){
 const children=new Map<string,CodexWatch[]>(),nested=new Set<string>();
 for(const child of watches){
  const parent=child.piParent;if(!child.threadId.startsWith('pi:')||!parent)continue;
  const parentId=parent.provider==='CLAUDE_CODE'?`claude_code:${parent.externalId}`:parent.provider==='CODEX'?parent.externalId:null;
  const original=watches.find(w=>w.threadId===parentId&&root(w.cwd)===root(parent.cwd));
  if(!original)continue;
  children.set(original.threadId,[...(children.get(original.threadId)??[]),child]);nested.add(child.threadId);
 }
 return {children,nested};
}
