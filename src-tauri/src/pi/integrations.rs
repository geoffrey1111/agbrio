//! Explicit desktop-owner allowlist. Native files stay in place; no discovery or credential copy.
use super::{resources::{Resources,RuleFile},session::plain};
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::{path::{Path,PathBuf},fs};
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Policy{version:u32,extensions:Vec<Extension>,mcp_servers:Vec<Server>}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Extension{path:String,sha256:String}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Server{name:String,url:String}
fn pin(path:&Path)->Result<RuleFile,String>{let p=fs::canonicalize(path).map_err(|_|"PI_INTEGRATION_SOURCE_UNAVAILABLE")?;let m=fs::metadata(&p).map_err(|_|"PI_INTEGRATION_SOURCE_UNAVAILABLE")?;if !m.is_file()||m.len()>2*1024*1024{return Err("PI_INTEGRATION_SOURCE_INVALID".into());}let bytes=fs::read(&p).map_err(|_|"PI_INTEGRATION_SOURCE_UNAVAILABLE")?;Ok(RuleFile{path:plain(&p),sha256:format!("{:x}",Sha256::digest(bytes)),via:"owner-allowlist".into(),required:true,source_path:None})}
pub(super) fn prepare(directory:&Path,resources:&mut Resources)->Result<(),String>{
 let base=PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("PI_OWNER_UNAVAILABLE")?).join("AIWorkRouter/data");let policy=base.join("pi-resources.json");if !policy.is_file(){return Ok(());}
 let home=PathBuf::from(std::env::var_os("USERPROFILE").ok_or("PI_OWNER_UNAVAILABLE")?);let extroot=fs::canonicalize(home.join(".pi/agent/extensions")).map_err(|_|"PI_INTEGRATION_SOURCE_UNAVAILABLE")?;
 let approved:Policy=serde_json::from_slice(&fs::read(&policy).map_err(|_|"PI_INTEGRATION_POLICY_INVALID")?).map_err(|_|"PI_INTEGRATION_POLICY_INVALID")?;if approved.version!=1||approved.extensions.len()>16||approved.mcp_servers.len()>16{return Err("PI_INTEGRATION_POLICY_INVALID".into());}
 resources.integration_sources.push(pin(&policy)?);
 for entry in approved.extensions{let file=pin(Path::new(&entry.path))?;let actual=fs::canonicalize(&file.path).map_err(|_|"PI_INTEGRATION_SOURCE_UNAVAILABLE")?;if !actual.starts_with(&extroot)||file.sha256!=entry.sha256.to_ascii_lowercase(){return Err("PI_EXTENSION_APPROVAL_CHANGED".into());}resources.extensions.push(file.clone());resources.integration_sources.push(file);}
 if !approved.mcp_servers.is_empty(){let config=pin(&home.join(".pi/agent/mcp.json"))?;let raw:serde_json::Value=serde_json::from_slice(&fs::read(&config.path).map_err(|_|"PI_MCP_CONFIG_INVALID")?).map_err(|_|"PI_MCP_CONFIG_INVALID")?;
  for server in approved.mcp_servers{if server.name.is_empty()||!server.name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){return Err("PI_MCP_APPROVAL_INVALID".into());}let url=url::Url::parse(&server.url).map_err(|_|"PI_MCP_APPROVAL_INVALID")?;if url.scheme()!="http"||url.host_str()!=Some("127.0.0.1")||!url.username().is_empty()||url.password().is_some()||url.query().is_some()||url.fragment().is_some(){return Err("PI_MCP_APPROVAL_INVALID".into());}if raw["mcpServers"][&server.name]["url"]!=server.url{return Err("PI_MCP_APPROVAL_CHANGED".into());}resources.mcp_servers.push(server.name);}
  let root=PathBuf::from(std::env::var_os("APPDATA").ok_or("PI_OWNER_UNAVAILABLE")?).join("npm/node_modules/@earendil-works/pi-coding-agent/dist/extensions/mcp");resources.mcp_module=Some(pin(&root.join("index.js"))?);resources.mcp_config_module=Some(pin(&root.join("config.js"))?);resources.mcp_config=Some(config.clone());resources.integration_sources.extend([config,resources.mcp_module.clone().unwrap(),resources.mcp_config_module.clone().unwrap()]);
 }
 for source in &resources.integration_sources{resources.protected_roots.push(source.path.clone());}
 let _=directory;Ok(())
}
