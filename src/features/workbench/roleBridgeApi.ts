import {desktopWatchChatApi} from "./watchChatApi";
import { invoke } from "@tauri-apps/api/core";
import { codexApi } from "../codex/api";
import type { ExistingCodexThreadCatalog } from "../codex/types";
import type { RoleBridgeApi, RoleBindings, RoleInput, RoleState, RoleHandoff } from "./RoleBridgePanel";

export const desktopRoleBridgeApi: RoleBridgeApi = {
 chat:desktopWatchChatApi,
 markRead:codexApi.markReplyObservationRead,
 openChat:async thread=>(await desktopWatchChatApi.state(thread)).watch,
 state: workstreamId => invoke<RoleState>("role_bridge_state",{workstreamId}),
 sync: workstreamId => invoke<RoleState>("sync_role_bridge",{workstreamId}),
 bind: (workstreamId,revision,decision:RoleInput,execution:RoleInput) => invoke<RoleBindings>("bind_role_bridge",{workstreamId,revision,decision,execution}),
 read: (workstreamId,role) => invoke<RoleState>("read_role_bridge",{workstreamId,role}),
 prepare: (workstreamId,role,observationId,text,attachmentIds = []) => invoke<RoleHandoff>("prepare_role_handoff",{workstreamId,role,observationId,text,attachmentIds}),
 blocks: (workstreamId,role,observationId) => invoke("role_bridge_blocks",{workstreamId,role,observationId}),
 attachments: (workstreamId,role,observationId) => invoke("role_bridge_attachments",{workstreamId,role,observationId}),
 edit: (handoffId,expectedHash,text) => invoke<RoleHandoff>("edit_role_handoff",{handoffId,expectedHash,text}),
 approve: (handoffId,expectedHash) => invoke<RoleHandoff>("approve_role_handoff",{handoffId,expectedHash}),
 send: handoffId => invoke<RoleState>("send_role_handoff",{handoffId}),
 threads: () => invoke<ExistingCodexThreadCatalog>("role_bridge_threads"),
 connect: () => codexApi.connect(),
};
