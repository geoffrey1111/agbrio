//! Native Pi context and skill resources, with owner-file provenance.
use super::{permissions,session::plain};
use router_core::store::{pi::Task,pi_policy::PiTaskAccess};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{fs,io::Read,path::{Path,PathBuf}};

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct RuleFile{pub path:String,pub sha256:String,#[serde(default)] pub via:String,#[serde(default)]pub required:bool,#[serde(skip_serializing_if="Option::is_none")]pub source_path:Option<String>}
#[derive(Clone,Default,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct Resources{pub instructions:Vec<RuleFile>,pub skill_roots:Vec<String>,pub skill_files:Vec<RuleFile>,pub append_file:Option<String>,pub append_digest:Option<String>,pub protected_roots:Vec<String>,pub credential_roots:Vec<String>,#[serde(default)]pub extensions:Vec<RuleFile>,#[serde(default)]pub integration_sources:Vec<RuleFile>,#[serde(default)]pub mcp_servers:Vec<String>,#[serde(default)]pub mcp_config:Option<RuleFile>,#[serde(default)]pub mcp_module:Option<RuleFile>,#[serde(default)]pub mcp_config_module:Option<RuleFile>}
impl Resources{
 pub fn unchanged(&self)->bool{self.instructions.iter().chain(self.integration_sources.iter()).all(|rule|{
  if let Some(source)=&rule.source_path{if fs::canonicalize(source).ok().is_none_or(|p|plain(&p)!=rule.path){return false;}}
  if fs::canonicalize(&rule.path).ok().is_none_or(|p|plain(&p)!=rule.path){return false;}
  let Ok(mut file)=fs::File::open(&rule.path)else{return false;};let mut bytes=Vec::new();if file.by_ref().take(2*1024*1024+1).read_to_end(&mut bytes).is_err()||bytes.len()>2*1024*1024{return false;}
  format!("{:x}",Sha256::digest(bytes))==rule.sha256
 })}
}
fn context(directory:&Path)->Option<PathBuf>{["AGENTS.override.md","AGENTS.md","AGENTS.MD","CLAUDE.md","CLAUDE.MD"].iter().map(|n|directory.join(n)).find(|p|p.is_file())}
fn file(path:&Path)->Result<(RuleFile,String),String>{
 let canonical=fs::canonicalize(path).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?;
 let canonical=PathBuf::from(plain(&canonical));
 if !canonical.extension().and_then(|s|s.to_str()).is_some_and(|s|s.eq_ignore_ascii_case("md")){return Err("PI_CONTEXT_REQUIRES_MARKDOWN".into());}
 let metadata=fs::metadata(&canonical).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?;
 if !metadata.is_file()||metadata.len()>128*1024{return Err("PI_CONTEXT_INVALID".into());}
 permissions::canonical_directory(&plain(canonical.parent().ok_or("PI_CONTEXT_INVALID")?))?;
 let mut bytes=Vec::new();fs::File::open(&canonical).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?.take(2*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?;if bytes.len()>128*1024{return Err("PI_CONTEXT_TOO_LARGE".into());}let text=String::from_utf8(bytes.clone()).map_err(|_|"PI_CONTEXT_REQUIRES_UTF8")?;
 Ok((RuleFile{path:plain(&canonical),sha256:format!("{:x}",Sha256::digest(bytes)),via:"native".into(),required:false,source_path:(plain(path)!=plain(&canonical)).then(||plain(path))},text))
}
fn skills(directory:&Path,output:&mut Vec<RuleFile>,seen:&mut usize,visited:&mut std::collections::HashSet<PathBuf>)->Result<(),String>{
 let actual=fs::canonicalize(directory).map_err(|_|"PI_SKILLS_UNAVAILABLE")?;if !visited.insert(actual){return Ok(());}
 let mut entries=fs::read_dir(directory).map_err(|_|"PI_SKILLS_UNAVAILABLE")?.collect::<Result<Vec<_>,_>>().map_err(|_|"PI_SKILLS_UNAVAILABLE")?;entries.sort_by_key(|v|v.file_name());
 for entry in entries{*seen+=1;if *seen>30000{return Err("PI_SKILL_CATALOG_TOO_LARGE".into());}
  let kind=entry.file_type().map_err(|_|"PI_SKILLS_UNAVAILABLE")?;
  if kind.is_symlink(){if entry.path().is_dir()&&entry.path().join("SKILL.md").is_file(){let target=fs::canonicalize(entry.path()).map_err(|_|"PI_SKILLS_UNAVAILABLE")?;let target=permissions::canonical_directory(&plain(&target))?;skills(&target,output,seen,visited)?;}continue;}
  if kind.is_dir(){skills(&entry.path(),output,seen,visited)?;}
  else if entry.file_name().to_string_lossy().eq_ignore_ascii_case("SKILL.md"){let (record,_)=file(&entry.path())?;if !output.iter().any(|v|v.path==record.path){output.push(record);}}
 }
 Ok(())
}
pub(super) fn prepare(task:&Task,access:&PiTaskAccess,owned:&Path)->Result<Resources,String>{
 let mut resources=Resources::default();resources.protected_roots.push(plain(owned));
 for name in["APPDATA","LOCALAPPDATA"]{if let Some(p)=std::env::var_os(name){resources.credential_roots.push(plain(&PathBuf::from(p)));}}
 if let Some(local)=std::env::var_os("LOCALAPPDATA"){resources.protected_roots.push(plain(&PathBuf::from(local).join("AIWorkRouter")));}
 if let Ok(exe)=std::env::current_exe(){if exe.file_name().and_then(|s|s.to_str())==Some("ai-work-router.exe"){if let Some(parent)=exe.parent(){resources.protected_roots.push(plain(parent));}}}
 let contract="Agbrio delegation: you are the Pi execution child, not the manager or owner. Execute only the manager's authorized task. Filesystem access is a ceiling, not permission to modify unrelated projects. Before editing another project or nested scope, read its applicable AGENTS/CLAUDE rules. Never approve your own result, alter acceptance standards, copy credentials or launch other model agents without explicit delegation. Keep this session's provider/model/thinking unchanged. Platform-specific tools named by inherited instructions are not automatically available; report missing capabilities.
";
 if access.writable&&access.terminal{super::integrations::prepare(Path::new(&task.directory),&mut resources)?;}
 if !access.inherit_context{return finish(owned,resources,contract);}
 let home=PathBuf::from(std::env::var_os("USERPROFILE").ok_or("PI_OWNER_UNAVAILABLE")?);
 let mut appendix=String::from(contract);
 let platform=if task.parent.provider=="CLAUDE_CODE"{".claude"}else{".codex"};
 let mut global=Vec::new();
 if let Some(p)=context(&home.join(".agents")){global.push(p);}
 if let Some(p)=context(&home.join(platform)){global.push(p);}
 let mut total=0usize;
 for path in global{let (mut record,text)=file(&path)?;record.via="append".into();total+=text.len();appendix.push_str(&format!("\nOwner global instructions ({})\n{}\n",record.path,text));resources.instructions.push(record);}
 if let Some(path)=context(&home.join(".pi/agent")){let(mut record,text)=file(&path)?;record.required=true;total+=text.len();resources.instructions.push(record);}
 let mut ancestors=Path::new(&task.directory).ancestors().map(Path::to_path_buf).collect::<Vec<_>>();ancestors.reverse();
 let project_boundary=ancestors.iter().rev().find(|p|p.join(".git").exists()).cloned();
 for directory in &ancestors{if let Some(path)=context(directory){let (record,text)=file(&path)?;total+=text.len();resources.instructions.push(record.clone());
   let parent_file=directory.join("CLAUDE.md");
   if platform==".claude"&&project_boundary.as_ref().is_none_or(|root|directory.starts_with(root))&&parent_file.is_file()&&parent_file!=path&&!directory.join("AGENTS.override.md").exists(){let(mut extra,body)=file(&parent_file)?;extra.via="append".into();total+=body.len();appendix.push_str(&format!("\nParent platform project guidance ({})\n{}\n",extra.path,body));resources.instructions.push(extra);}
  }}
 if let Some(nearest)=resources.instructions.iter_mut().rev().find(|r|r.via=="native"){nearest.required=true;}
 if total>512*1024||resources.instructions.len()>40{return Err("PI_CONTEXT_TOO_LARGE".into());}
 let mut candidates=Vec::new();
 for directory in ancestors.into_iter().rev(){for name in [".agents/skills",".pi/skills"]{candidates.push(directory.join(name));}candidates.push(directory.join(platform).join("skills"));candidates.push(directory.join(if platform==".claude"{".codex"}else{".claude"}).join("skills"));}
 candidates.extend([home.join(".agents/skills"),home.join(platform).join("skills"),home.join(if platform==".claude"{".codex/skills"}else{".claude/skills"}),home.join(".pi/agent/skills")]);
 for path in candidates{if path.is_dir(){let canonical=fs::canonicalize(&path).map_err(|_|"PI_SKILLS_UNAVAILABLE")?;let canonical=permissions::canonical_directory(&plain(&canonical))?;let path=plain(&canonical);if !resources.skill_roots.contains(&path){resources.skill_roots.push(path);}}}
 let mut seen=0;let mut visited=std::collections::HashSet::new();for root in &resources.skill_roots{skills(Path::new(root),&mut resources.skill_files,&mut seen,&mut visited)?;}

 if resources.skill_files.len()>256{return Err("PI_SKILL_CATALOG_TOO_LARGE".into());}
 if resources.skill_roots.len()>24{return Err("PI_CONTEXT_TOO_LARGE".into());}
 finish(owned,resources,&appendix)
}
fn finish(owned:&Path,mut resources:Resources,appendix:&str)->Result<Resources,String>{
 let append=owned.join("agbrio-context.md");if fs::symlink_metadata(&append).is_ok_and(|m|m.file_type().is_symlink()){return Err("PI_CONTEXT_INVALID".into());}
 fs::write(&append,appendix).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?;resources.append_file=Some(plain(&append));resources.append_digest=Some(format!("{:x}",Sha256::digest(fs::read(&append).map_err(|_|"PI_CONTEXT_UNAVAILABLE")?)));
 Ok(resources)
}
