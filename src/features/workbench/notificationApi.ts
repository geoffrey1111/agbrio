import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { codexApi } from "../codex/api";
import { request } from "../../mobile/api";
import type { NotificationApi } from "./CodexNotifications";
import { desktopWatchChatApi, webWatchChatApi } from "./watchChatApi";
import {readModels} from "./readModelCache";
export function cachedNotificationApi(raw:NotificationApi):NotificationApi {
 const update=async<T>(work:()=>Promise<T>)=>{const value=await work();readModels.invalidate("notifications:");return value;};
 return {...raw,cachedWatches:()=>readModels.peek("notifications:watches"),cachedFeed:()=>readModels.peek("notifications:feed"),subscribe:fn=>{const a=readModels.subscribe("notifications:watches",fn),b=readModels.subscribe("notifications:feed",fn);return()=>{a();b();};},
  watches:()=>readModels.read("notifications:watches",raw.watches),feed:after=>after?raw.feed(after):readModels.read("notifications:feed",()=>raw.feed(0)),event:id=>readModels.read(`notifications:event:${id}`,()=>raw.event(id),30000),
  enable:id=>update(()=>raw.enable(id)),pause:id=>update(()=>raw.pause(id)),remove:raw.remove?(...args)=>update(()=>raw.remove!(...args)):undefined,
  markSeen:raw.markSeen?id=>update(()=>raw.markSeen!(id)):undefined,markRead:raw.markRead?id=>update(()=>raw.markRead!(id)):undefined,
 };
}
export const desktopNotificationApi:NotificationApi=cachedNotificationApi({
 markSeen:sequence=>invoke("codex_watch_mark_seen",{sequence}),
 markRead:sequence=>invoke("codex_watch_mark_read",{sequence}),
 chat:desktopWatchChatApi,
 delivery:{desktop:true,settings:()=>invoke("codex_delivery_settings"),command:input=>invoke("codex_delivery_command",{input})},
 onOpen:async callback=>{const dispose=await listen<number>("codex-notification-open",e=>callback(e.payload));try{const pending=await invoke<number|null>("codex_notification_navigation");if(pending!==null)callback(pending);return dispose;}catch(e){dispose();throw e;}},
 webUrl:()=>invoke("codex_notifications_web_url"),
 remove:(kind,id,removed)=>invoke("codex_watch_remove",{kind,id,removed}),watches:()=>invoke("codex_watch_list"),connect:()=>codexApi.connect(),threads:()=>invoke("codex_watch_candidates"),enable:threadId=>invoke("codex_watch_enable",{threadId}),pause:threadId=>invoke("codex_watch_pause",{threadId}),feed:after=>invoke("codex_watch_feed",{after}),event:sequence=>invoke("codex_watch_event",{sequence}),
});
function command<T>(body:unknown){return request<T>("/codex-watches",{method:"POST",body:JSON.stringify(body)});}
export const webNotificationApi:NotificationApi=cachedNotificationApi({
 markSeen:sequence=>command({action:"MARK_SEEN",sequence}),
 markRead:sequence=>command({action:"MARK_READ",sequence}),
 chat:webWatchChatApi,
 delivery:{desktop:false,settings:()=>request("/codex-watches/delivery"),command:input=>request("/codex-watches/delivery",{method:"POST",body:JSON.stringify(input)})},
 webUrl:async()=>`${location.origin}/mobile/notifications`,
 remove:(kind,id,removed)=>command({action:"REMOVE",kind,id,removed}),watches:()=>request("/codex-watches"),connect:()=>command({action:"CONNECT"}),threads:()=>command({action:"THREADS"}),enable:threadId=>command({action:"ENABLE",threadId}),pause:threadId=>command({action:"PAUSE",threadId}),feed:after=>request(`/codex-watches/events?after=${after}`),event:sequence=>request(`/codex-watches/events/${sequence}`),
});
