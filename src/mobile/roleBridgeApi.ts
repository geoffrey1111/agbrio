import {webNotificationApi} from "../features/workbench/notificationApi";
import{webWatchChatApi}from"../features/workbench/watchChatApi";
import { request } from "./api";
import type { RoleBridgeApi, RoleState } from "../features/workbench/RoleBridgePanel";

function roleCommand<T>(workstreamId:string,body:unknown){return request<T>(`/workstreams/${encodeURIComponent(workstreamId)}/role-bridge`,{method:"POST",body:JSON.stringify(body)});}
// Scope the instance to its selected Workstream; never use global selection for routing.
export function mobileRoleBridgeApi(workstreamId:string): RoleBridgeApi {
 return {
  chat:webWatchChatApi,
  openChat:async thread=>{let all=await webNotificationApi.watches();if(!all.some(w=>w.threadId===thread&&w.enabled)){await webNotificationApi.enable(thread);all=await webNotificationApi.watches();}const watch=all.find(w=>w.threadId===thread);if(!watch)throw Error("未能打开对话");return watch;},
  state: async id => request<RoleState>(`/workstreams/${encodeURIComponent(id)}/role-bridge`),
  sync: id => roleCommand(id,{action:"SYNC"}),
  bind: (id,revision,decision,execution) => roleCommand(id,{action:"BIND",revision,decision,execution}),
  read: (id,role) => roleCommand(id,{action:"READ",role}),
  prepare: (id,role,observationId,text,attachmentIds = []) => roleCommand(id,{action:"PREPARE",role,observationId,text,attachmentIds}),
  blocks: (id,role,observationId) => roleCommand(id,{action:"BLOCKS",role,observationId}),
  attachments: (id,role,observationId) => roleCommand(id,{action:"ATTACHMENTS",role,observationId}),
  edit: (handoffId,expectedHash,text) => roleCommand(workstreamId,{action:"EDIT",handoffId,expectedHash,text}),
  approve: (handoffId,expectedHash) => roleCommand(workstreamId,{action:"APPROVE",handoffId,expectedHash}),
  send: handoffId => roleCommand(workstreamId,{action:"SEND",handoffId}),
  threads: id => roleCommand(id,{action:"THREADS"}),
  connect: () => roleCommand(workstreamId,{action:"CONNECT"}),
 };
}
