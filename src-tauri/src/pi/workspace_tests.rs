//! Opt-in real native acceptance. All work belongs to isolated owned fixtures.
use super::{application,session::plain,workspaces};
use crate::host_application::RouterCore;
use router_core::store::{RouterStore,assistant::AssistantConnectionInput};
use serde_json::{json,Value};
use std::{path::{Path,PathBuf},sync::{Arc,Mutex},time::{Duration,Instant}};

struct Fixture{core:RouterCore,root:PathBuf,project:PathBuf,shared:PathBuf,grant:String,parent:String,scope:String,generation:i64,rule:String,skill:String}
impl Drop for Fixture{fn drop(&mut self){self.core.shutdown_owned_adapter();}}
fn fixture()->Fixture{fixture_mode(true)}
fn fixture_mode(administrator:bool)->Fixture{
 assert_eq!(std::env::var("AGBRIO_PI_NATIVE_QA").as_deref(),Ok("1"));
 let base=PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));assert!(base.is_absolute()&&base.is_dir());
 let root=base.join(format!("workspace-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
 let project=root.join("project");let shared=root.join("shared");std::fs::create_dir(&project).unwrap();std::fs::create_dir(&shared).unwrap();
 let rule=format!("RULE_{}",uuid::Uuid::new_v4().simple());let skill=format!("SKILL_{}",uuid::Uuid::new_v4().simple());
 std::fs::write(project.join("AGENTS.md"),format!("# Isolated native acceptance\nFor the workspace acceptance task, include {rule} in the final result. Only the explicitly named fixture files may be changed.\n")).unwrap();
 std::fs::write(shared.join("AGENTS.md"),"Only beta-output.txt may be modified in this secondary fixture project.\n").unwrap();
 let skill_dir=project.join(".agents/skills/agbrio-workspace-probe");std::fs::create_dir_all(&skill_dir).unwrap();
 std::fs::write(skill_dir.join("SKILL.md"),format!("---\nname: agbrio-workspace-probe\ndescription: Run the isolated Agbrio workspace file and tool acceptance probe.\n---\nRead this skill before the acceptance task. Include {skill} in the final response. Do not install anything or use network, git or other model agents.\n")).unwrap();
 let store=Arc::new(RouterStore::open_at(root.join("router.db")).unwrap());
 let core=RouterCore{store:store.clone(),session:Arc::new(Mutex::default()),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};
 let parent=uuid::Uuid::new_v4().to_string();store.enable_codex_watch(&parent,"DISPOSABLE FIXTURE, NOT GENUINE MANAGER",&plain(&project),&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
 let at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()as i64;
 let grant=store.connect_assistant_instance(AssistantConnectionInput{label:"Owned Pi workspace QA".into(),expires_at:at+900000}).unwrap();
 workspaces::owner_authorize(&core,workspaces::WorkspaceInput{roots:vec![plain(&project),plain(&shared)],machine_scope:false,writable:true,terminal:true,administrator,inherit_context:true,grant_ids:vec![grant.id.clone()]}).unwrap();
 let scope=store.pi_scopes().unwrap().into_iter().find(|s|router_core::claude_code::same_root(&s.root,&plain(&project))).unwrap();
 Fixture{core,root,project,shared,grant:grant.id,parent,scope:scope.id,generation:scope.generation,rule,skill}
}
fn call(f:&Fixture,name:&str,args:Value)->Value{crate::assistant_mcp::call(&f.core,&f.grant,name,args).unwrap()}
fn reviewed(f:&Fixture,request:&str,operation:&str,input:Value)->Value{
 let action=call(f,"agbrio_prepare_action",json!({"requestId":request,"operation":operation,"input":input}));
 let result=call(f,"agbrio_execute_action",json!({"actionId":action["id"],"expectedHash":action["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Owner-authorized isolated Pi integration acceptance. Touch only these disposable fixtures; preserve all real parent conversations and projects."}));
 assert_eq!(result["status"],"APPLIED","{operation}: {}",result["errorCode"]);result["result"].clone()
}
fn create(f:&Fixture,writable:bool)->Value{
 reviewed(f,"create-owned-workspace","CREATE_CHAT",json!({"provider":"PI","directory":plain(&f.project),"label":"Pi workspace acceptance","pi":{"scopeId":f.scope,"scopeGeneration":f.generation,"parent":{"provider":"CODEX","externalId":f.parent,"cwd":plain(&f.project)},"accessRoots":[plain(&f.project),plain(&f.shared)],"writable":writable,"modelProvider":"anthropic","model":"claude-haiku-5-5","thinking":"low"}}))
}
fn close(f:&Fixture,id:&str,request:&str){let view=call(f,"agbrio_read_chat",json!({"threadId":id}));let result=reviewed(f,request,"STOP_CHAT",json!({"threadId":id,"turnId":view["task"]["runtimeEpoch"]}));assert_eq!(result["closed"],true);assert_eq!(call(f,"agbrio_read_chat",json!({"threadId":id}))["runtimeLoaded"],false);}

#[test]
#[ignore="real installed Pi metadata and administrator token; explicit owned QA only"]
fn native_workspace_creation_is_create_only_and_readonly_removes_terminal(){
 let f=fixture();let created=create(&f,false);let id=created["threadId"].as_str().unwrap();
 assert_eq!(created["firstMessageSent"],false);assert_eq!(created["execution"]["process"]["administrator"],true);
 assert_eq!(created["execution"]["access"]["writable"],false);assert_eq!(created["execution"]["access"]["terminal"],false);
 assert!(created["execution"]["nativeSkills"].as_array().unwrap().iter().any(|s|s["name"].as_str().is_some_and(|n|n.contains("agbrio-workspace-probe"))));
 let view=call(&f,"agbrio_read_chat",json!({"threadId":id}));assert_eq!(view["task"]["state"],"IDLE");assert_eq!(view["execution"]["contextApplied"],false);assert!(view["history"]["messages"].as_array().unwrap().is_empty());
 close(&f,id,"close-metadata-only");
 std::fs::write(f.root.join("metadata-proof.json"),serde_json::to_vec_pretty(&created).unwrap()).unwrap();
 eprintln!("Native workspace metadata/token/source catalog/create-only/readonly tools PASS: {}",plain(&f.root));
}

#[test]
#[ignore="paid native file/terminal/context/skill acceptance; exact owned QA only"]
fn real_workspace_files_terminal_rules_skills_and_closed_access_restore(){
 assert_eq!(std::env::var("AGBRIO_PI_PAID_QA").as_deref(),Ok("1"));let f=fixture();let created=create(&f,true);let id=created["threadId"].as_str().unwrap();
 let text=format!("Perform only this isolated workspace acceptance. Use the advertised agbrio-workspace-probe skill, reading its SKILL.md first. Use write to create alpha.txt with exactly 'before', then use edit to change it to exactly 'after'. Before writing the secondary project, read its AGENTS.md. Use write to create {} with exactly 'secondary-ok'. Use the powershell tool to verify the current Windows token is administrator, write terminal.txt with exactly 'terminal-ok', and write environment variable NAMES ONLY to env-names.txt. Do not output environment values. Do not install anything, access network, run git or start other model agents. Return a brief result including the applicable rule and skill markers.",plain(&f.shared.join("beta-output.txt")));
 reviewed(&f,"send-owned-workspace","SEND_CHAT",json!({"threadId":id,"generation":created["generation"],"sourceSequence":null,"expectedTurnId":null,"mode":"AUTO","options":{"attachments":[]},"text":text}));
 let until=Instant::now()+Duration::from_secs(150);let result=loop{let view=call(&f,"agbrio_read_chat",json!({"threadId":id}));if matches!(view["task"]["state"].as_str(),Some("RESULT_READY"|"FAILED"|"PARTIAL"|"CANCELLED")){break view;}assert!(Instant::now()<until,"owned native task timed out");std::thread::sleep(Duration::from_millis(250));};
 assert_eq!(result["task"]["state"],"RESULT_READY","native task error: {}",result["task"]["errorCode"]);
 let last=&result["task"]["result"]["lastAssistant"];let output=last["content"].as_array().unwrap().iter().filter_map(|b|b["text"].as_str()).collect::<Vec<_>>().join("\n");assert!(output.contains(&f.rule)&&output.contains(&f.skill),"native rule/skill consumption missing");
 for(path,expected)in[(f.project.join("alpha.txt"),"after"),(f.shared.join("beta-output.txt"),"secondary-ok"),(f.project.join("terminal.txt"),"terminal-ok")]{assert_eq!(std::fs::read_to_string(path).unwrap().trim(),expected);}
 let names=std::fs::read_to_string(f.project.join("env-names.txt")).unwrap();assert!(!names.contains("AGBRIO_PI_")&&!names.contains("OPENAI_API_KEY")&&!names.contains("ANTHROPIC_API_KEY"));
 let tools=result["history"]["messages"].as_array().unwrap().iter().filter_map(|m|m["toolName"].as_str()).collect::<Vec<_>>();for name in["read","write","edit","powershell"]{assert!(tools.contains(&name),"missing actual native tool {name}");}
 assert_eq!(result["execution"]["contextApplied"],true);assert!(result["execution"]["modelRequestAttempts"].as_u64().unwrap()>0);
 close(&f,id,"close-completed-workspace");
 let restored=reviewed(&f,"restore-workspace-readonly","ENABLE_WATCH",json!({"threadId":id,"piAccess":{"generation":created["generation"],"scopeId":f.scope,"scopeGeneration":f.generation,"accessRoots":[plain(&f.project)],"writable":false}}));
 assert_eq!(restored["threadId"],id);assert_eq!(restored["firstMessageSent"],false);assert_eq!(restored["execution"]["access"]["terminal"],false);assert_eq!(restored["execution"]["contextApplied"],false);
 assert!(restored["generation"].as_i64().unwrap()>created["generation"].as_i64().unwrap());
 close(&f,id,"close-restored-workspace");
 let proof=json!({"fixtureOnly":true,"genuineManager":false,"threadId":id,"created":created,"nativeTerminalReceiptId":result["task"]["result"]["nativeTerminalReceiptId"],"contextApplied":result["execution"]["contextApplied"],"ruleMarker":f.rule,"skillMarker":f.skill,"tools":tools,"managementAccessRestore":restored,"environmentPrivateNamesAbsent":true});
 std::fs::write(f.root.join("workspace-proof.json"),serde_json::to_vec_pretty(&proof).unwrap()).unwrap();eprintln!("Real native workspace file/terminal/context/skill/restore PASS: {}",plain(&f.root));
}

#[test]
#[ignore="real Windows UAC consent/cancel from a standard owner process; metadata only"]
fn standard_owner_uac_metadata_or_explicit_cancel(){
 assert_eq!(std::env::var("AGBRIO_PI_NATIVE_QA").as_deref(),Ok("1"));
 let parent=super::elevation::identity(None).unwrap();assert!(!parent.administrator,"must launch this probe from the real standard desktop Explorer token");
 let root=PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").unwrap());assert!(root.is_absolute()&&root.is_dir());
 let project=root.join(format!("uac-project-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&project).unwrap();
 let base=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("AIWorkRouter/data/pi-executor");std::fs::create_dir_all(&base).unwrap();let owned=base.join(uuid::Uuid::new_v4().to_string());std::fs::create_dir(&owned).unwrap();
 let access=router_core::store::pi_policy::PiTaskAccess{roots:vec![plain(&project)],writable:false,terminal:false,administrator:true,inherit_context:false};
 let resources=super::resources::Resources::default();let selection=router_core::store::pi::Selection{provider:"anthropic".into(),model:"claude-haiku-5-5".into(),thinking:"low".into()};
 std::fs::write(root.join("uac-probe-started.json"),serde_json::to_vec(&json!({"pid":std::process::id(),"parentAdministrator":false,"owned":plain(&owned),"cancelExpected":std::env::var("AGBRIO_PI_EXPECT_CANCEL").as_deref()==Ok("1")})).unwrap()).unwrap();
 let prepared=super::cohort::prepare();
 if std::env::var("AGBRIO_PI_EXPECT_CANCEL").as_deref()!=Ok("1"){prepared.as_ref().unwrap();assert_eq!(super::cohort::status()["ready"],true);}
 let opened=if prepared.is_ok(){super::session::Session::open_scoped(&super::installation().unwrap(),&project,&owned,false,false,Some(&selection),None,None,Some((&access,&resources)))}else{Err(prepared.unwrap_err())};
 if std::env::var("AGBRIO_PI_EXPECT_CANCEL").as_deref()==Ok("1"){
  assert!(matches!(opened,Err(ref error)if error=="PI_UAC_CANCELLED"),"expected actual native UAC cancellation");
  std::fs::write(root.join("uac-cancel-proof.json"),serde_json::to_vec_pretty(&json!({"parentAdministrator":false,"nativeCancellation":true,"ordinaryFallback":false,"firstMessageSent":false})).unwrap()).unwrap();
 }else{
  let session=opened.unwrap();let process=session.rpc.process_identity().unwrap();assert!(process.administrator);assert_eq!(process.owner_sid,parent.owner_sid);assert_eq!(session.selected.provider,selection.provider);assert_eq!(session.selected.model,selection.model);assert_eq!(session.selected.thinking,selection.thinking);
  let history=session.history().unwrap();assert!(!history["entries"].as_array().unwrap().iter().any(|e|e["type"]=="message"&&e["message"]["role"]=="user"));
  session.close().unwrap();drop(session);
  let second_owned=base.join(uuid::Uuid::new_v4().to_string());std::fs::create_dir(&second_owned).unwrap();
  let second=super::session::Session::open_scoped(&super::installation().unwrap(),&project,&second_owned,false,false,Some(&selection),None,None,Some((&access,&resources))).unwrap();
  assert!(second.rpc.process_identity().unwrap().administrator);second.close().unwrap();drop(second);std::fs::remove_dir_all(&second_owned).unwrap();
  super::cohort::shutdown();assert_eq!(super::cohort::status()["ready"],false);
  std::fs::write(root.join("uac-admin-proof.json"),serde_json::to_vec_pretty(&json!({"parentAdministrator":false,"child":process,"sameOwnerSid":true,"firstMessageSent":false,"closeConfirmed":true,"cohortReusedForTwoChildren":true})).unwrap()).unwrap();
 }
 // This UUID was created by this probe and has no user conversation or model task.
 assert_eq!(owned.parent(),Some(base.as_path()));assert!(uuid::Uuid::parse_str(owned.file_name().unwrap().to_str().unwrap()).is_ok());std::fs::remove_dir_all(&owned).unwrap();
 eprintln!("Owned standard-token UAC metadata probe PASS");
}

#[test]
#[ignore="native metadata-only concurrent workspace coordinator fence"]
fn same_manager_siblings_share_workspace_other_manager_is_fenced(){
 let f=fixture();let first=create(&f,true);
 let input=json!({"provider":"PI","directory":plain(&f.project),"label":"Second coordinated Pi child","pi":{"scopeId":f.scope,"scopeGeneration":f.generation,"parent":{"provider":"CODEX","externalId":f.parent,"cwd":plain(&f.project)},"writable":true,"modelProvider":"anthropic","model":"claude-haiku-5-5","thinking":"low"}});
 let second=reviewed(&f,"create-second-coordinated-child","CREATE_CHAT",input.clone());assert_ne!(first["threadId"],second["threadId"]);
 let foreign=uuid::Uuid::new_v4().to_string();f.core.store.enable_codex_watch(&foreign,"DISPOSABLE OTHER COORDINATOR",&plain(&f.project),&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
 let mut other=input;other["pi"]["parent"]["externalId"]=json!(foreign);
 let action=call(&f,"agbrio_prepare_action",json!({"requestId":"create-foreign-overlap","operation":"CREATE_CHAT","input":other}));
 let result=call(&f,"agbrio_execute_action",json!({"actionId":action["id"],"expectedHash":action["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"Owner-authorized isolated metadata overlap fence, no model prompt."}));
 assert_eq!(result["errorCode"],"PI_DIRECTORY_WRITER_ALREADY_ACTIVE");
 close(&f,first["threadId"].as_str().unwrap(),"close-first-coordinated");close(&f,second["threadId"].as_str().unwrap(),"close-second-coordinated");
}

#[test]
#[ignore="real installed Pi metadata-only cross-volume policy admission; no model task"]
fn metadata_creation_on_secondary_drive_of_machine_workspace(){
 let mut f=fixture();assert!(plain(&f.project).starts_with("D:"));let original_scope=f.scope.clone();let original_generation=f.generation;
 workspaces::owner_authorize(&f.core,workspaces::WorkspaceInput{roots:vec![],machine_scope:true,writable:true,terminal:true,administrator:true,inherit_context:true,grant_ids:vec![f.grant.clone()]}).unwrap();
 let scope=f.core.store.pi_scopes().unwrap().into_iter().find(|v|v.root.starts_with("C:")).unwrap();let policy=f.core.store.pi_workspace_policy(&scope.id).unwrap().unwrap();assert!(policy.roots.iter().any(|p|p.starts_with("D:")));f.scope=scope.id;f.generation=scope.generation;
 let created=create(&f,false);assert_eq!(created["firstMessageSent"],false);assert_eq!(created["execution"]["process"]["administrator"],true);assert!(created["directory"].as_str().unwrap().starts_with("D:"));
 close(&f,created["threadId"].as_str().unwrap(),"close-cross-volume-metadata");
 f.scope=original_scope;f.generation=original_generation;
 let secondary=reviewed(&f,"create-secondary-root-cwd","CREATE_CHAT",json!({"provider":"PI","directory":plain(&f.shared),"label":"Secondary-root metadata only","pi":{"scopeId":f.scope,"scopeGeneration":f.generation,"parent":{"provider":"CODEX","externalId":f.parent,"cwd":plain(&f.project)},"accessRoots":[plain(&f.project),plain(&f.shared)],"writable":false,"modelProvider":"anthropic","model":"claude-haiku-5-5","thinking":"low"}}));assert_eq!(secondary["firstMessageSent"],false);assert!(router_core::claude_code::same_root(secondary["directory"].as_str().unwrap(),&plain(&f.shared)));close(&f,secondary["threadId"].as_str().unwrap(),"close-secondary-root-metadata");
 std::fs::write(f.root.join("cross-volume-proof.json"),serde_json::to_vec_pretty(&json!({"machinePolicyCPrimaryDCwd":created,"sameDriveSecondaryRootCwd":secondary,"modelPrompts":0})).unwrap()).unwrap();eprintln!("Actual native C-primary/D-cwd and secondary-root create-only PASS: {}",plain(&f.root));
}

#[test]
#[ignore="real native Pi + approved local MCP tool discovery; zero model prompts"]
fn native_integrations_metadata_only(){
 let f=fixture_mode(super::elevation::identity(None).unwrap().administrator);let created=create(&f,true);let id=created["threadId"].as_str().unwrap();
 assert_eq!(created["firstMessageSent"],false);let pi=f.core.session.lock().unwrap().pi.clone();let task=f.core.store.pi_task(id).unwrap();let audit=pi.audit(&task.id).unwrap();
 assert!(audit["capabilities"]["mcpReady"].as_bool()==Some(true),"MCP readiness missing: {}",audit["capabilities"]);
 let tools=audit["capabilities"]["tools"].as_array().unwrap();let count=tools.iter().filter(|t|t.as_str().is_some_and(|n|n.starts_with("mcp__unityMCP__"))).count();assert!(count>0);
 assert!(audit["resources"]["extensions"].as_array().unwrap().iter().any(|e|e["path"].as_str().is_some_and(|p|p.ends_with("unity-popup-hook.ts"))));
 let read=call(&f,"agbrio_read_chat",json!({"threadId":id}));assert!(read["history"]["messages"].as_array().unwrap().is_empty());close(&f,id,"close-approved-integrations");
 std::fs::write(f.root.join("integrations-refresh-proof.json"),serde_json::to_vec_pretty(&json!({"realNativeSessions":1,"unityMcpTools":count,"ownerHookLoaded":true,"modelPrompts":0,"businessFileWrites":0,"closed":true})).unwrap()).unwrap();eprintln!("INTEGRATIONS_REFRESH_METADATA_PASS {}",plain(&f.root));
}

#[test]
#[ignore="one owner-authorized minimum-cost native Pi model call using readonly Unity MCP"]
fn native_integrations_model_readonly_call(){
 assert_eq!(std::env::var("AGBRIO_PI_PAID_QA").as_deref(),Ok("1"));
 let f=fixture_mode(super::elevation::identity(None).unwrap().administrator);
 std::fs::write(f.project.join("AGENTS.md"),"# Isolated read-only MCP acceptance\nUse only the single read_console get call named in the task. No filesystem writes, terminal, agents, Unity modification, console clear, or other tool calls.\n").unwrap();
 let created=create(&f,true);let id=created["threadId"].as_str().unwrap();let task=f.core.store.pi_task(id).unwrap();
 let pi=f.core.session.lock().unwrap().pi.clone();let audit=pi.audit(&task.id).unwrap();
 assert_eq!(audit["capabilities"]["toolSetSource"],"ACTIVE_MODEL");
 let names=audit["capabilities"]["tools"].as_array().unwrap();
 assert!(names.iter().any(|v|v=="mcp__unityMCP__read_console"));
 assert!(!names.iter().any(|v|v=="bash"||v=="list_mcp_resources"));
 reviewed(&f,"readonly-unity-model-tool-probe","SEND_CHAT",json!({"threadId":id,"generation":task.generation,"mode":"AUTO","sourceSequence":null,"text":"Read-only tool connectivity test. Call mcp__unityMCP__read_console exactly once with action=get, count=1, types=[error], format=plain, include_stacktrace=false. Do not clear anything, write files, use terminal, call other tools, or modify Unity. After successful tool return reply only PI_UNITY_ACTIVE_TOOL_OK. Do not quote console content.","options":{}}));
 let deadline=Instant::now()+Duration::from_secs(180);
 loop{let t=f.core.store.pi_task(id).unwrap();if matches!(t.state.as_str(),"RESULT_READY"|"FAILED"){break;}assert!(Instant::now()<deadline,"Single submitted model probe remains uncertain; do not resend");std::thread::sleep(Duration::from_millis(250));}
 let settled=f.core.store.pi_task(id).unwrap();
 let entries=std::fs::read_to_string(settled.native_session_file.as_ref().unwrap()).unwrap().lines().map(|l|serde_json::from_str::<Value>(l).unwrap()).collect::<Vec<_>>();
 let calls=entries.iter().filter(|e|e["customType"]=="agbrio.tool_authorized").collect::<Vec<_>>();
 let results=entries.iter().filter(|e|e["type"]=="message"&&e["message"]["role"]=="toolResult").collect::<Vec<_>>();
 // Close our temporary process before asserting the transport outcome.
 close(&f,id,"close-readonly-unity-model-probe");
 assert_eq!(settled.state,"RESULT_READY");assert_eq!(calls.len(),1);assert_eq!(calls[0]["data"]["toolName"],"mcp__unityMCP__read_console");
 assert_eq!(results.len(),1);assert_eq!(results[0]["message"]["toolName"],"mcp__unityMCP__read_console");assert_ne!(results[0]["message"]["isError"],true);
 let tool_args=entries.iter().filter_map(|e|e["message"]["content"].as_array()).flatten().find(|b|b["type"]=="toolCall").unwrap()["arguments"].clone();
 assert_eq!(tool_args["action"],"get");assert!(tool_args["count"]==1||tool_args["count"]=="1");
 std::fs::write(f.root.join("model-active-mcp-proof.json"),serde_json::to_vec_pretty(&json!({"nativeSessionId":settled.native_session_id,"model":settled.selected,"toolSetSource":"ACTIVE_MODEL","activeMcpTools":names.iter().filter(|n|n.as_str().is_some_and(|s|s.starts_with("mcp__unityMCP__"))).count(),"modelPrompts":1,"nativeAuthorizedToolCalls":1,"toolName":"mcp__unityMCP__read_console","toolArguments":tool_args,"nativeToolResultError":false,"finalState":settled.state,"closed":true,"businessFileWrites":0})).unwrap()).unwrap();
 eprintln!("MODEL_ACTIVE_MCP_PASS {}",plain(&f.root));
}

#[test]
#[ignore="one minimum-cost owned native Pi prompt, then failed idle context refresh on same session"]
fn native_failed_context_refresh_minimum_cost(){
 assert_eq!(std::env::var("AGBRIO_PI_PAID_QA").as_deref(),Ok("1"));let f=fixture_mode(super::elevation::identity(None).unwrap().administrator);let created=create(&f,false);let id=created["threadId"].as_str().unwrap();let pi=f.core.session.lock().unwrap().pi.clone();let original=f.core.store.pi_task(id).unwrap();let old_epoch=original.runtime_epoch.clone();
 reviewed(&f,"minimal-context-probe","SEND_CHAT",json!({"threadId":id,"generation":original.generation,"mode":"AUTO","sourceSequence":null,"text":format!("Only return PI_CONTEXT_REFRESH_QA and {}. Do not use tools, read files, call MCP, change settings or launch agents.",f.rule),"options":{}}));
 let deadline=Instant::now()+Duration::from_secs(180);loop{let t=f.core.store.pi_task(id).unwrap();if matches!(t.state.as_str(),"RESULT_READY"|"FAILED"){break;}assert!(Instant::now()<deadline,"Accepted test remains uncertain; never resend");std::thread::sleep(Duration::from_millis(300));}
 let settled=f.core.store.pi_task(id).unwrap();assert!(Path::new(settled.native_session_file.as_deref().unwrap()).is_file());let initial_state=settled.state.clone();
 f.core.store.pi_set_state(&original.id,"FAILED",None,Some("ISOLATED_CONTEXT_REFRESH_CASE")).unwrap();std::fs::write(f.project.join("AGENTS.md"),"# Updated isolated rules\nNo new model task may run in this context refresh.\n").unwrap();
 pi.refresh_failed_context(f.core.store.clone(),&original.id).unwrap();let restored=f.core.store.pi_task(id).unwrap();assert_eq!(restored.native_session_id,original.native_session_id);assert_ne!(restored.runtime_epoch,old_epoch);assert_eq!(restored.generation,original.generation+1);assert_eq!(restored.selected.as_ref().unwrap().model,original.selected.as_ref().unwrap().model);close(&f,id,"close-refreshed-failure");
 std::fs::write(f.root.join("failed-context-refresh-proof.json"),serde_json::to_vec_pretty(&json!({"sameNativeSession":true,"runtimeEpochChanged":true,"generationAdvancedForFreshReview":true,"modelUnchanged":true,"initialProviderState":initial_state,"modelPrompts":1,"businessFileWrites":0,"closed":true})).unwrap()).unwrap();eprintln!("FAILED_CONTEXT_REFRESH_PASS {}",plain(&f.root));
}

#[test]
#[ignore="resume exact existing owned paid QA fixture without another model prompt"]
fn native_context_refresh_existing_fixture_without_model(){
 let root=PathBuf::from(std::env::var_os("AGBRIO_PI_EXISTING_FIXTURE").expect("exact owned QA fixture"));let base=PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").unwrap()).canonicalize().unwrap();let root=root.canonicalize().unwrap();assert!(root.starts_with(&base)&&root.file_name().unwrap().to_str().unwrap().starts_with("workspace-"));
 let store=Arc::new(RouterStore::open_at(root.join("router.db")).unwrap());let core=RouterCore{store:store.clone(),session:Arc::new(Mutex::default()),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};let pi=core.session.lock().unwrap().pi.clone();let original=store.pi_tasks().unwrap().remove(0);let native=original.native_session_id.clone();let first=pi.start(store.clone(),&original.id,true).unwrap();
 store.pi_set_state(&first.id,"FAILED",None,Some("ISOLATED_CONTEXT_REFRESH_CASE")).unwrap();std::fs::write(Path::new(&first.directory).join("AGENTS.md"),format!("# Isolated renewed rules {}\nMetadata-only recovery; no model prompt or business file operation is authorized.\n",uuid::Uuid::new_v4())).unwrap();
 let request=uuid::Uuid::new_v4().to_string();let err=pi.send(store.clone(),&first.id,request.clone(),first.generation,"AUTO".into(),"This explicit preflight probe must not be submitted".into(),Vec::new()).unwrap_err();assert_eq!(err,"PI_CONTEXT_REFRESHED_READ_CHAT_AND_PREPARE_NEW_ACTION_NOT_SENT");assert!(store.pi_command_receipt(&first.id,&request).is_err());let after=store.pi_task(&first.id).unwrap();assert_eq!(after.native_session_id,native);assert_ne!(after.runtime_epoch,first.runtime_epoch);assert_eq!(after.generation,first.generation+1);assert_eq!(after.scope_generation,first.scope_generation);pi.close_task(&store,&first.id,&uuid::Uuid::new_v4().to_string(),after.generation).unwrap();
 let fresh=super::PiService::default();let cold_request=uuid::Uuid::new_v4().to_string();let cold_error=fresh.send(store.clone(),&first.id,cold_request.clone(),after.generation,"AUTO".into(),"This cold-load preflight probe must not be submitted".into(),Vec::new()).unwrap_err();assert_eq!(cold_error,"PI_SESSION_RELOADED_READ_CHAT_AND_PREPARE_NEW_ACTION_NOT_SENT");let cold=store.pi_task(&first.id).unwrap();assert_eq!(cold.native_session_id,native);assert_eq!(cold.generation,after.generation+1);assert!(fresh.loaded(&first.id));assert!(store.pi_command_receipt(&first.id,&cold_request).is_err());fresh.close_task(&store,&first.id,&uuid::Uuid::new_v4().to_string(),cold.generation).unwrap();
 core.shutdown_owned_adapter();std::fs::write(root.join("failed-context-refresh-proof.json"),serde_json::to_vec_pretty(&json!({"sameNativeSession":true,"runtimeEpochChanged":true,"generationAdvancedForFreshReview":true,"modelUnchanged":after.selected.as_ref().unwrap().model==first.selected.as_ref().unwrap().model,"coldCompletedSessionAutoReloaded":true,"oldGenerationInputNotSubmitted":true,"additionalModelPrompts":0,"priorModelPrompts":1,"businessFileWrites":0,"closed":true})).unwrap()).unwrap();eprintln!("FAILED_CONTEXT_REFRESH_PASS {}",plain(&root));
}
