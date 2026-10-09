import type {RoleBridgeApi,RoleState} from "./RoleBridgePanel";
import {readModels,roleCacheKey} from "./readModelCache";
/** Sync never writes provider input. All user commands keep their original
 * exact revision/hash checks; their returned presentation replaces the cache. */
export function cachedRoleBridgeApi(raw:RoleBridgeApi):RoleBridgeApi {
 const key=roleCacheKey;
 const load=(id:string,operation:()=>Promise<RoleState>)=>async()=>{const value=await operation();if(value.bindings.workstreamId!==id)throw Error("BRIDGE_BINDING_CHANGED");return value;};
 return {...raw,
  cachedState:id=>readModels.peek(key(id)),subscribeState:(id,fn)=>readModels.subscribe(key(id),fn),
  warmState:id=>readModels.warm(key(id),load(id,()=>raw.state(id)),30000),
  markRead:raw.markRead?async(id,observation)=>{await raw.markRead!(id,observation);const old=readModels.peek<RoleState>(key(id));if(old)readModels.put(key(id),{...old,replies:old.replies.map(reply=>reply.id===observation?{...reply,readAt:Date.now()}:reply)});}:undefined,
  state:id=>readModels.read(key(id),load(id,()=>raw.state(id)),15000),
  sync:raw.sync?id=>readModels.read(key(id),load(id,()=>raw.sync!(id)),2000):undefined,
  read:async(id,role)=>{const value=await raw.read(id,role);readModels.put(key(id),value);return value;},
  bind:async(id,...args)=>{const value=await raw.bind(id,...args);readModels.invalidate(key(id),true);readModels.invalidate(`workstream:${id}`,true);readModels.invalidate("directory:");return value;},
  send:async id=>{const value=await raw.send(id);readModels.put(key(value.bindings.workstreamId),value);readModels.invalidate("directory:");readModels.invalidate("notifications:");return value;},
 };
}
