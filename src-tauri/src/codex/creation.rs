//! Exact new-thread model settings. No global config or permission writes.
use super::adapter::CodexAdapter;
use serde_json::{json,Value};

#[derive(Default,Clone)]
pub(crate) struct Settings {pub model:Option<String>,pub effort:Option<String>,pub service_tier:Option<String>}
impl Settings {
 pub fn requested(&self)->Value{json!({"model":self.model,"effort":self.effort,"serviceTier":self.service_tier})}
 pub fn any(&self)->bool{self.model.is_some()||self.effort.is_some()||self.service_tier.is_some()}
 pub fn shape(&self)->Result<(),String>{
  for (field,value) in [("model",&self.model),("effort",&self.effort),("serviceTier",&self.service_tier)]{if value.as_ref().is_some_and(|s|s.is_empty()||s.len()>200||s.trim()!=s||s.chars().any(char::is_control)){return Err(format!("ASSISTANT_CREATE_SETTINGS_INVALID: CREATE_CHAT.{field} must be a nonempty catalog value"));}}
  if self.model.is_none()&&(self.effort.is_some()||self.service_tier.is_some()){return Err("ASSISTANT_CREATE_SETTINGS_INVALID: CREATE_CHAT.model is required when selecting effort or serviceTier; call agbrio_list_codex_models".into());}Ok(())
 }
}
pub(crate) fn models(adapter:&mut CodexAdapter)->Result<Value,String>{
 let mut data=Vec::new();let mut cursor=Value::Null;let mut seen=std::collections::HashSet::new();
 for _ in 0..20 {
  let page=adapter.request("model/list",json!({"limit":100,"includeHidden":false,"cursor":cursor}))?;
  let rows=page["data"].as_array().ok_or("CODEX_MODEL_CATALOG_UNAVAILABLE")?;
  for row in rows {if row["hidden"]==true{continue;}let Some(id)=row["id"].as_str()else{return Err("CODEX_MODEL_CATALOG_INVALID".into());};let model=row["model"].as_str().unwrap_or(id);
   data.push(json!({"id":id,"model":model,"displayName":row["displayName"],"supportedReasoningEfforts":row["supportedReasoningEfforts"],"defaultReasoningEffort":row["defaultReasoningEffort"],"serviceTiers":row["serviceTiers"].as_array().cloned().unwrap_or_default(),"defaultServiceTier":row["defaultServiceTier"],"isDefault":row["isDefault"]}));
  }
  cursor=page.get("nextCursor").cloned().unwrap_or(Value::Null);if cursor.is_null(){return Ok(json!({"models":data,"standardServiceTier":"default","standardNativeValue":null,"omittedSettings":"INHERIT_NATIVE_DEFAULTS","scope":"CREATE_CHAT_ONLY","startsModelTurn":false}));}
  let next=cursor.as_str().ok_or("CODEX_MODEL_CATALOG_INVALID")?;if !seen.insert(next.to_owned()){return Err("CODEX_MODEL_CATALOG_CURSOR_LOOP".into());}
 }Err("CODEX_MODEL_CATALOG_PAGE_LIMIT".into())
}
pub(crate) fn validate(adapter:&mut CodexAdapter,settings:&Settings)->Result<(),String>{
 settings.shape()?;if !settings.any(){return Ok(());}
 let catalog=models(adapter)?;let model=catalog["models"].as_array().unwrap().iter().find(|m|settings.model.as_deref()==m["model"].as_str()).ok_or("ASSISTANT_CREATE_MODEL_UNAVAILABLE: call agbrio_list_codex_models")?;
 if let Some(effort)=&settings.effort {if !model["supportedReasoningEfforts"].as_array().is_some_and(|rows|rows.iter().any(|e|e["reasoningEffort"].as_str()==Some(effort))){return Err("ASSISTANT_CREATE_EFFORT_UNAVAILABLE: use this model's supportedReasoningEfforts".into());}}
 if let Some(tier)=&settings.service_tier {if tier!="default"&&!model["serviceTiers"].as_array().is_some_and(|rows|rows.iter().any(|t|t["id"].as_str()==Some(tier))){return Err("ASSISTANT_CREATE_SPEED_UNAVAILABLE: use default or this model's serviceTiers[].id".into());}}Ok(())
}
pub(crate) fn start(adapter:&mut CodexAdapter,cwd:&std::path::Path,settings:&Settings)->Result<Value,String>{
 validate(adapter,settings)?;let mut params=json!({"cwd":cwd,"ephemeral":false});
 if let Some(model)=&settings.model {params["model"]=json!(model);params["allowProviderModelFallback"]=json!(false);}
 if let Some(effort)=&settings.effort {params["config"]=json!({"model_reasoning_effort":effort});}
 if let Some(tier)=&settings.service_tier {params["serviceTier"]=if tier=="default"{Value::Null}else{json!(tier)};}
 adapter.request("thread/start",params)
}
pub(crate) fn effective(response:&Value)->Value{json!({"model":response["model"],"effort":response["reasoningEffort"],"serviceTier":response["serviceTier"]})}
pub(crate) fn verify(response:&Value,settings:&Settings)->Result<(),String>{
 if settings.model.as_ref().is_some_and(|v|response["model"].as_str()!=Some(v))||settings.effort.as_ref().is_some_and(|v|response["reasoningEffort"].as_str()!=Some(v))||settings.service_tier.as_ref().is_some_and(|v|if v=="default"{response.get("serviceTier")!=Some(&Value::Null)&&response["serviceTier"].as_str()!=Some("default")}else{response["serviceTier"].as_str()!=Some(v)}){return Err("ASSISTANT_CREATE_SETTINGS_UNCONFIRMED_CHECK_RECEIPT".into());}Ok(())
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn standard_tier_native_canonicalization_is_not_an_uncertain_creation(){let s=Settings{model:Some("m".into()),effort:Some("high".into()),service_tier:Some("default".into())};for tier in [Value::Null,json!("default")]{assert!(verify(&json!({"model":"m","reasoningEffort":"high","serviceTier":tier}),&s).is_ok());}assert!(verify(&json!({"model":"m","reasoningEffort":"high"}),&s).is_err());assert!(verify(&json!({"model":"m","reasoningEffort":"high","serviceTier":"priority"}),&s).is_err());}
}
