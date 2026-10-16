//! Rotating public-client OAuth credentials. No raw token is persisted.
use super::*;
use super::assistant::AssistantGrant;

pub struct OAuthTokens {pub access_token:String,pub refresh_token:String,pub expires_at:i64,pub scope:String}
fn token()->String{format!("{}{}",Uuid::new_v4().simple(),Uuid::new_v4().simple())}
fn hash(value:&str)->String{format!("{:x}",Sha256::digest(value.as_bytes()))}
fn insert(c:&Connection,g:&AssistantGrant,client:&str,resource:&str,family:&str,deadline:i64)->Result<OAuthTokens,String>{
 let access=token();let refresh=token();let expires=deadline.min(now()+3600_000);
 c.execute("INSERT INTO assistant_access(token_hash,grant_id,resource,expires_at) VALUES(?1,?2,?3,?4)",params![hash(&access),g.id,resource,expires]).map_err(db_error)?;
 c.execute("INSERT INTO assistant_oauth_refresh(token_hash,grant_id,client_id,resource,expires_at,family_id,access_hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![hash(&refresh),g.id,client,resource,deadline,family,hash(&access)]).map_err(db_error)?;
 Ok(OAuthTokens{access_token:access,refresh_token:refresh,expires_at:expires,scope:if g.scope=="INSTANCE"{"agbrio:instance"}else{"agbrio:handoff"}.into()})
}
impl RouterStore{
 pub fn issue_assistant_oauth_tokens(&self,gid:&str,client:&str,resource:&str)->Result<OAuthTokens,String>{
  self.with_connection(|c|{
   if client.is_empty()||client.len()>128||resource.len()>1024{return Err("ASSISTANT_OAUTH_INVALID_REQUEST".into());}
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let g=super::assistant::active(&tx,gid)?;
   // Retain used-token identities until the original lease ends for replay detection.
   tx.execute("DELETE FROM assistant_oauth_refresh WHERE expires_at<=?1",[now()]).map_err(db_error)?;
   let count:i64=tx.query_row("SELECT COUNT(*) FROM assistant_oauth_refresh WHERE client_id=?1",[client],|r|r.get(0)).map_err(db_error)?;
   if count>=10000{return Err("ASSISTANT_OAUTH_TOKEN_LIMIT".into());}
   let result=insert(&tx,&g,client,resource,&id(),g.expires_at)?;tx.commit().map_err(db_error)?;Ok(result)
  })
 }
 pub fn refresh_assistant_oauth_tokens(&self,refresh:&str,client:&str,resource:&str,scope:Option<&str>)->Result<OAuthTokens,String>{
  if refresh.len()!=64{return Err("ASSISTANT_OAUTH_INVALID_GRANT".into());}
  self.with_connection(|c|{
   let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
   let row=tx.query_row("SELECT grant_id,family_id,expires_at,used_at,revoked_at FROM assistant_oauth_refresh WHERE token_hash=?1 AND client_id=?2 AND resource=?3",params![hash(refresh),client,resource],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<i64>>(4)?))).optional().map_err(db_error)?.ok_or("ASSISTANT_OAUTH_INVALID_GRANT")?;
   if row.3.is_some(){
    // Correctly-bound replay revokes only this refresh family, never a grant or
    // another assistant's independently issued credentials/subscriptions.
    tx.execute("UPDATE assistant_oauth_refresh SET revoked_at=COALESCE(revoked_at,?2) WHERE family_id=?1",params![row.1,now()]).map_err(db_error)?;
    tx.execute("DELETE FROM assistant_access WHERE token_hash IN(SELECT access_hash FROM assistant_oauth_refresh WHERE family_id=?1)",[&row.1]).map_err(db_error)?;
    tx.commit().map_err(db_error)?;return Err("ASSISTANT_OAUTH_REFRESH_REPLAY".into());
   }
   if row.4.is_some()||row.2<=now(){return Err("ASSISTANT_OAUTH_INVALID_GRANT".into());}
   let g=super::assistant::active(&tx,&row.0)?;
   let expected=if g.scope=="INSTANCE"{"agbrio:instance"}else{"agbrio:handoff"};
   if scope.is_some_and(|s|s!=expected){return Err("ASSISTANT_OAUTH_SCOPE_INVALID".into());}
   let count:i64=tx.query_row("SELECT COUNT(*) FROM assistant_oauth_refresh WHERE client_id=?1",[client],|r|r.get(0)).map_err(db_error)?;
   if count>=10000{return Err("ASSISTANT_OAUTH_TOKEN_LIMIT".into());}
   tx.execute("UPDATE assistant_oauth_refresh SET used_at=?2 WHERE token_hash=?1",params![hash(refresh),now()]).map_err(db_error)?;
   let result=insert(&tx,&g,client,resource,&row.1,row.2.min(g.expires_at))?;tx.commit().map_err(db_error)?;Ok(result)
  })
 }
}

#[cfg(test)]mod tests{
 use super::*;
 const RESOURCE:&str="https://assistant.fixture.invalid/mcp";
 fn fixture()->(tempfile::TempDir,RouterStore,String){
  let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();
  let g=s.connect_assistant_instance(super::super::assistant::AssistantConnectionInput{label:"OAuth fixture".into(),expires_at:now()+600000}).unwrap();(d,s,g.id)
 }
 #[test]fn rotating_credentials_survive_restart_keep_absolute_lease_and_never_store_raw_tokens(){
  let(d,s,gid)=fixture();let first=s.issue_assistant_oauth_tokens(&gid,"workbuddy",RESOURCE).unwrap();
  assert!(s.authenticate_assistant(&first.refresh_token,RESOURCE).is_err());
  s.with_connection(|c|{let raw:String=c.query_row("SELECT token_hash FROM assistant_oauth_refresh",[],|r|r.get(0)).map_err(db_error)?;assert!(raw!=first.refresh_token);Ok(())}).unwrap();
  let deadline=s.assistant_grant(&gid).unwrap().expires_at;drop(s);
  let s=RouterStore::open_at(d.path().join("router.db")).unwrap();
  let second=s.refresh_assistant_oauth_tokens(&first.refresh_token,"workbuddy",RESOURCE,None).unwrap();
  assert!(second.refresh_token!=first.refresh_token);assert!(second.expires_at<=deadline);assert_eq!(second.scope,"agbrio:instance");
  assert!(s.authenticate_assistant(&second.access_token,RESOURCE).is_ok());
  let lease:i64=s.with_connection(|c|c.query_row("SELECT expires_at FROM assistant_oauth_refresh WHERE token_hash=?1",[hash(&second.refresh_token)],|r|r.get(0)).map_err(db_error)).unwrap();assert_eq!(lease,deadline);
 }
 #[test]fn wrong_client_resource_or_scope_never_consumes_or_expands_refresh(){
  let(_d,s,gid)=fixture();let first=s.issue_assistant_oauth_tokens(&gid,"client",RESOURCE).unwrap();
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"other",RESOURCE,None).is_err());
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"client","https://other.invalid/mcp",None).is_err());
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"client",RESOURCE,Some("agbrio:instance agbrio:handoff")).is_err());
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"client",RESOURCE,Some("agbrio:instance")).is_ok());
 }
 #[test]fn replay_revokes_only_its_family_preserving_legacy_dot_and_other_clients(){
  let(_d,s,gid)=fixture();let dot=s.issue_assistant_access(&gid,RESOURCE,s.assistant_grant(&gid).unwrap().expires_at).unwrap();
  let a=s.issue_assistant_oauth_tokens(&gid,"workbuddy",RESOURCE).unwrap();let other=s.issue_assistant_oauth_tokens(&gid,"other",RESOURCE).unwrap();
  let b=s.refresh_assistant_oauth_tokens(&a.refresh_token,"workbuddy",RESOURCE,None).unwrap();
  assert!(s.refresh_assistant_oauth_tokens(&a.refresh_token,"workbuddy",RESOURCE,None).is_err());
  assert!(s.authenticate_assistant(&a.access_token,RESOURCE).is_err());assert!(s.authenticate_assistant(&b.access_token,RESOURCE).is_err());
  assert!(s.refresh_assistant_oauth_tokens(&b.refresh_token,"workbuddy",RESOURCE,None).is_err());
  assert!(s.authenticate_assistant(&dot,RESOURCE).is_ok());assert!(s.authenticate_assistant(&other.access_token,RESOURCE).is_ok());
 }
 #[test]fn expired_or_revoked_authorization_cannot_refresh(){
  let(_d,s,gid)=fixture();let first=s.issue_assistant_oauth_tokens(&gid,"client",RESOURCE).unwrap();s.revoke_assistant_grant(&gid).unwrap();
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"client",RESOURCE,None).is_err());
  let(_d,s,gid)=fixture();let first=s.issue_assistant_oauth_tokens(&gid,"client",RESOURCE).unwrap();
  s.with_connection(|c|{c.execute("UPDATE assistant_oauth_refresh SET expires_at=?1",[now()-1]).map_err(db_error)?;Ok(())}).unwrap();
  assert!(s.refresh_assistant_oauth_tokens(&first.refresh_token,"client",RESOURCE,None).is_err());
 }
 #[test]fn concurrent_refresh_can_succeed_only_once(){
  let(_d,s,gid)=fixture();let first=s.issue_assistant_oauth_tokens(&gid,"client",RESOURCE).unwrap();let s=std::sync::Arc::new(s);let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
  let jobs:Vec<_>=(0..2).map(|_|{let s=s.clone();let b=barrier.clone();let token=first.refresh_token.clone();std::thread::spawn(move||{b.wait();s.refresh_assistant_oauth_tokens(&token,"client",RESOURCE,None).is_ok()})}).collect();
  assert_eq!(jobs.into_iter().filter_map(|j|j.join().ok()).filter(|ok|*ok).count(),1);
 }
}
