import {invoke} from "@tauri-apps/api/core";
import {request} from "../../mobile/api";
export type MediaScope={kind:"WATCH";threadId:string;sequence?:number|null;turnId?:string|null;itemId?:string|null}|{kind:"ROLE";workstreamId:string;role:string;observationId:string};
export type MediaFile={filename:string;mime:string;data:string};
export type MediaResolver=((source:string,userRequested?:boolean)=>Promise<MediaFile>)&{scopeKey?:string};
const resolvers=new Map<string,MediaResolver>();
export function messageMedia(scope:MediaScope):MediaResolver{const key=JSON.stringify(scope);const old=resolvers.get(key);if(old)return old;const resolver:MediaResolver=(source,userRequested=false)=>location.pathname.startsWith("/mobile")?request("/media",{method:"POST",body:JSON.stringify({scope,source,userRequested})}):invoke("read_message_media",{input:{scope,source,userRequested}});resolver.scopeKey=key;if(resolvers.size>=512)resolvers.delete(resolvers.keys().next().value!);resolvers.set(key,resolver);return resolver;}
