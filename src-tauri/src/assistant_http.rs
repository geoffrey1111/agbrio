//! Independent MCP bearer audience and owner-consented OAuth/PKCE.
//! Temporary OAuth exchanges are bounded and fail closed on Host restart.
use super::*;
use axum::{extract::Form,response::{Response,Html,Redirect}};
use base64ct::{Base64UrlUnpadded,Encoding};
use sha2::{Digest,Sha256};
use std::collections::HashMap;

const SCOPE:&str="agbrio:handoff";
const INSTANCE_SCOPE:&str="agbrio:instance";
// OAuth scope is a space-delimited set, not one enum value. A client may
// request both advertised scopes; owner consent still selects one existing
// grant, and the token returns only that grant's scope (never their union).
fn valid_scope_request(scope:&str)->bool{
 !scope.is_empty()&&scope.len()<=128&&scope.split(' ').all(|s|s==SCOPE||s==INSTANCE_SCOPE)
}
fn scope_allows_grant(scope:&str,grant_scope:&str)->bool{
 let required=match grant_scope{"INSTANCE"=>INSTANCE_SCOPE,"BRIDGE"=>SCOPE,_=>return false};
 scope.is_empty()||scope.split(' ').any(|s|s==required)
}
fn clock()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()}
fn secret()->String{format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple())}
fn digest(s:&str)->String{format!("{:x}",Sha256::digest(s.as_bytes()))}
#[derive(Clone,Serialize,Deserialize)]struct Client{name:String,redirects:Vec<String>,expires:u64}
#[derive(Clone)]struct Intent{client:Client,client_id:String,redirect:String,challenge:String,state:String,resource:String,scope:String,expires:u64}
struct Code{intent:Intent,grant_id:String}
#[derive(Default)]pub(super) struct AssistantOAuth{clients:HashMap<String,Client>,intents:HashMap<String,Intent>,codes:HashMap<String,Code>,path:Option<PathBuf>,fault:bool}
impl AssistantOAuth{
 pub(super) fn open(path:PathBuf)->Self{
    let mut auth=Self{path:Some(path.clone()),..Self::default()};
    if path.exists(){match fs::read(&path).ok().and_then(|b|serde_json::from_slice::<HashMap<String,Client>>(&b).ok()){
        Some(clients) if clients.len()<=128&&clients.values().all(|c|!c.name.is_empty()&&c.name.len()<=120&&!c.redirects.is_empty()&&c.redirects.len()<=8&&c.redirects.iter().all(|r|redirect_allowed(r)))=>auth.clients=clients,
        _=>auth.fault=true,
    }}auth.prune();auth
 }
 fn save(&self)->Result<(),ApiError>{if let Some(path)=&self.path{
    fs::create_dir_all(path.parent().ok_or_else(bad)?).map_err(|_|ApiError(StatusCode::SERVICE_UNAVAILABLE,"ASSISTANT_AUTH_STORE_UNAVAILABLE".into()))?;
    let temporary=path.with_extension("pending");let bytes=serde_json::to_vec(&self.clients).map_err(|_|bad())?;
    fs::write(&temporary,bytes).and_then(|_|fs::rename(temporary,path)).map_err(|_|ApiError(StatusCode::SERVICE_UNAVAILABLE,"ASSISTANT_AUTH_STORE_UNAVAILABLE".into()))?;
 }Ok(())}
 fn prune(&mut self){let time=clock();self.clients.retain(|_,c|c.expires>time);self.intents.retain(|_,v|v.expires>time);self.codes.retain(|_,c|c.intent.expires>time);}
}
fn oauth(state:&MobileHttpState)->Result<std::sync::MutexGuard<'_,AssistantOAuth>,ApiError>{let auth=state.assistant_oauth.lock().map_err(|_|ApiError(StatusCode::SERVICE_UNAVAILABLE,"ASSISTANT_AUTH_UNAVAILABLE".into()))?;if auth.fault{return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE,"ASSISTANT_AUTH_STORE_INVALID".into()));}Ok(auth)}
fn boundary(headers:&HeaderMap,state:&MobileHttpState)->Result<(),ApiError>{
 pairing_boundary(headers,state,false)?;
 if headers.get("origin").is_some_and(|v|v.to_str().ok()!=Some(state.config.allowed_origin.as_str())){return Err(ApiError(StatusCode::FORBIDDEN,"Origin is not allowed".into()));}Ok(())
}
fn bad()->ApiError{ApiError(StatusCode::BAD_REQUEST,"ASSISTANT_OAUTH_REQUEST_INVALID".into())}
fn resource(state:&MobileHttpState)->String{format!("{}/mcp",state.config.allowed_origin.trim_end_matches('/'))}
pub(super) async fn metadata(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<serde_json::Value>{
 boundary(&headers,&state)?;let origin=&state.config.allowed_origin;
 Ok(Json(serde_json::json!({"resource":resource(&state),"authorization_servers":[origin],"scopes_supported":[SCOPE,INSTANCE_SCOPE],"bearer_methods_supported":["header"]})))
}
pub(super) async fn authorization_metadata(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<serde_json::Value>{
 boundary(&headers,&state)?;let origin=&state.config.allowed_origin;
 Ok(Json(serde_json::json!({"issuer":origin,"authorization_endpoint":format!("{origin}/oauth/authorize"),"token_endpoint":format!("{origin}/oauth/token"),"registration_endpoint":format!("{origin}/oauth/register"),"response_types_supported":["code"],"grant_types_supported":["authorization_code"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"scopes_supported":[SCOPE,INSTANCE_SCOPE],"client_id_metadata_document_supported":false})))
}
#[derive(Deserialize)]pub(super) struct Registration{client_name:String,redirect_uris:Vec<String>,token_endpoint_auth_method:Option<String>,grant_types:Option<Vec<String>>,response_types:Option<Vec<String>>}
fn redirect_allowed(value:&str)->bool{
 let Ok(u)=url::Url::parse(value)else{return false};
 u.username().is_empty()&&u.password().is_none()&&u.fragment().is_none()&&u.host_str().is_some()&&value.len()<2048&&
 (u.scheme()=="https"||(u.scheme()=="http"&&matches!(u.host_str(),Some("127.0.0.1"|"[::1]"|"localhost"))))
}
pub(super) async fn register(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<Registration>)->Result<(StatusCode,Json<serde_json::Value>),ApiError>{
 boundary(&headers,&state)?;
 // Codex/rmcp requests authorization_code + refresh_token even when the
 // server advertises authorization_code only. RFC7591 allows negotiation:
 // return only the supported grant below; never issue/advertise refresh tokens.
 if input.client_name.trim().is_empty()||input.client_name.len()>120||input.redirect_uris.is_empty()||input.redirect_uris.len()>8||!input.redirect_uris.iter().all(|v|redirect_allowed(v))||input.token_endpoint_auth_method.as_deref().is_some_and(|m|m!="none")||input.grant_types.as_ref().is_some_and(|g|!g.iter().any(|v|v=="authorization_code")||g.len()>2||g.iter().any(|v|v!="authorization_code"&&v!="refresh_token")||(g.len()==2&&g[0]==g[1]))||input.response_types.as_ref().is_some_and(|g|g!=&["code"]){return Err(bad());}
 let mut auth=oauth(&state)?;auth.prune();if auth.clients.len()>=128{return Err(ApiError(StatusCode::TOO_MANY_REQUESTS,"ASSISTANT_CLIENT_LIMIT".into()));}
 let id=uuid::Uuid::new_v4().to_string();auth.clients.insert(id.clone(),Client{name:input.client_name.clone(),redirects:input.redirect_uris.clone(),expires:clock()+3600});
 if let Err(e)=auth.save(){auth.clients.remove(&id);return Err(e);}
 Ok((StatusCode::CREATED,Json(serde_json::json!({"client_id":id,"client_id_issued_at":clock(),"client_name":input.client_name,"redirect_uris":input.redirect_uris,"token_endpoint_auth_method":"none","grant_types":["authorization_code"],"response_types":["code"]}))))
}
#[derive(Deserialize)]pub(super) struct Authorization{client_id:String,redirect_uri:String,response_type:String,code_challenge:String,code_challenge_method:String,state:String,resource:String,scope:Option<String>}
pub(super) async fn authorize(State(state):State<MobileHttpState>,headers:HeaderMap,Query(input):Query<Authorization>)->Result<Redirect,ApiError>{
 boundary(&headers,&state)?;let mut auth=oauth(&state)?;auth.prune();let client=auth.clients.get(&input.client_id).cloned().ok_or_else(bad)?;
 if input.response_type!="code"||input.code_challenge_method!="S256"||input.code_challenge.len()!=43||!input.code_challenge.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_')||!client.redirects.contains(&input.redirect_uri)||input.resource!=resource(&state)||input.scope.as_deref().is_some_and(|s|!valid_scope_request(s))||input.state.is_empty()||input.state.len()>2048{return Err(bad());}
 if auth.intents.len()>=128{return Err(ApiError(StatusCode::TOO_MANY_REQUESTS,"ASSISTANT_AUTHORIZATION_LIMIT".into()));}
 let id=secret();auth.intents.insert(id.clone(),Intent{client,client_id:input.client_id,redirect:input.redirect_uri,challenge:input.code_challenge,state:input.state,resource:input.resource,scope:input.scope.unwrap_or_default(),expires:clock()+300});
 Ok(Redirect::to(&format!("/assistant/connect?request={id}")))
}
pub(super) async fn page(State(state):State<MobileHttpState>,headers:HeaderMap)->Result<Response,ApiError>{
 boundary(&headers,&state)?;let nonce=secret();let html=include_str!("assistant_consent.html").replace("{{NONCE}}",&nonce);
 let mut response=Html(html).into_response();
 response.headers_mut().insert("content-security-policy",format!("default-src 'none'; script-src 'nonce-{nonce}'; style-src 'nonce-{nonce}'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'").parse().unwrap());
 response.headers_mut().insert("referrer-policy",HeaderValue::from_static("no-referrer"));
 response.headers_mut().insert("cache-control",HeaderValue::from_static("no-store"));Ok(response)
}
#[derive(Deserialize)]pub(super) struct IntentQuery{request:String}
pub(super) async fn consent_info(State(state):State<MobileHttpState>,headers:HeaderMap,Query(input):Query<IntentQuery>)->ApiResult<serde_json::Value>{
 authenticated(&headers,&state,false).await?;
 let intent={let mut auth=oauth(&state)?;auth.prune();auth.intents.get(&input.request).cloned().ok_or_else(bad)?};
 let grants=state.core.store.assistant_grants().map_err(core_error)?.into_iter().filter(|g|state.core.store.assistant_grant(&g.id).is_ok()).filter(|g|scope_allows_grant(&intent.scope,&g.scope)).map(|g|{
    let b=if g.scope=="INSTANCE"{None}else{Some(state.core.store.role_bridge(&g.workstream_id)?)};
    Ok(serde_json::json!({"grant":g,"bindings":b,"scope":g.scope}))
 }).collect::<Result<Vec<_>,String>>().map_err(core_error)?;
 Ok(Json(serde_json::json!({"clientName":intent.client.name,"redirectUri":intent.redirect,"grants":grants})))
}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]pub(super) struct Consent{request:String,grant_id:Option<String>,allow:bool}
pub(super) async fn consent(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<Consent>)->ApiResult<serde_json::Value>{
 authenticated(&headers,&state,true).await?;let mut auth=oauth(&state)?;auth.prune();
 let intent=auth.intents.get(&input.request).cloned().ok_or_else(bad)?;
 let mut redirect=url::Url::parse(&intent.redirect).map_err(|_|bad())?;
 if input.allow{
    let gid=input.grant_id.ok_or_else(bad)?;let g=state.core.store.assistant_grant(&gid).map_err(core_error)?;
    if !scope_allows_grant(&intent.scope,&g.scope){return Err(bad());}
    if let Some(c)=auth.clients.get_mut(&intent.client_id){c.expires=clock()+90*86400;}auth.save()?;
    let code=secret();redirect.query_pairs_mut().append_pair("code",&code).append_pair("state",&intent.state);
    let mut code_intent=intent;code_intent.expires=clock()+60;
    auth.codes.insert(digest(&code),Code{intent:code_intent,grant_id:gid});
 }else{redirect.query_pairs_mut().append_pair("error","access_denied").append_pair("state",&intent.state);}
 auth.intents.remove(&input.request);
 Ok(Json(serde_json::json!({"redirect":redirect.as_str()})))
}
#[derive(Deserialize)]pub(super) struct Exchange{grant_type:String,code:String,client_id:String,redirect_uri:String,code_verifier:String,resource:String}
pub(super) async fn token(State(state):State<MobileHttpState>,headers:HeaderMap,Form(input):Form<Exchange>)->Result<Response,ApiError>{
 boundary(&headers,&state)?;let mut auth=oauth(&state)?;auth.prune();
 let code=auth.codes.get(&digest(&input.code)).ok_or_else(bad)?;
 if input.grant_type!="authorization_code"||input.client_id!=code.intent.client_id||input.redirect_uri!=code.intent.redirect||input.resource!=code.intent.resource||input.resource!=resource(&state)||!(43..=128).contains(&input.code_verifier.len())||!input.code_verifier.bytes().all(|b|b.is_ascii_alphanumeric()||b"-._~".contains(&b))||Base64UrlUnpadded::encode_string(&Sha256::digest(input.code_verifier.as_bytes()))!=code.intent.challenge{return Err(bad());}
 let grant=state.core.store.assistant_grant(&code.grant_id).map_err(core_error)?;
 // Tokens are revocable via the owner grant, expire no later than it, and never
 // authenticate human/device routes. Only their one-way hashes are persisted.
 let access=state.core.store.issue_assistant_access(&grant.id,&input.resource,grant.expires_at).map_err(core_error)?;
 auth.codes.remove(&digest(&input.code));
 let mut response=Json(serde_json::json!({"access_token":access,"token_type":"Bearer","expires_in":(grant.expires_at/1000-clock() as i64).max(0),"scope":if grant.scope=="INSTANCE"{INSTANCE_SCOPE}else{SCOPE}})).into_response();
 response.headers_mut().insert("cache-control",HeaderValue::from_static("no-store"));response.headers_mut().insert("pragma",HeaderValue::from_static("no-cache"));Ok(response)
}
pub(super) async fn mcp(State(state):State<MobileHttpState>,headers:HeaderMap,Json(input):Json<serde_json::Value>)->Result<Response,ApiError>{
 boundary(&headers,&state)?;
 let credential=headers.get("authorization").and_then(|v|v.to_str().ok()).and_then(|v|v.strip_prefix("Bearer ")).unwrap_or("");
 let grant=match state.core.store.authenticate_assistant(credential,&resource(&state)){
    Ok(g)=>g,Err(_)=>{let mut response=(StatusCode::UNAUTHORIZED,Json(serde_json::json!({"error":"invalid_token"}))).into_response();response.headers_mut().insert("www-authenticate",format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"{INSTANCE_SCOPE}\"",state.config.allowed_origin).parse().map_err(|_|bad())?);return Ok(response);}
 };
 if let Some(v)=headers.get("mcp-protocol-version"){if !matches!(v.to_str().ok(),Some("2025-11-25"|"2025-06-18"|"2025-03-26")){return Err(bad());}}
 let accept=headers.get("accept").and_then(|v|v.to_str().ok()).unwrap_or("");
 if !accept.contains("application/json")||!accept.contains("text/event-stream"){return Err(ApiError(StatusCode::NOT_ACCEPTABLE,"MCP requires JSON and event-stream Accept types".into()));}
 if input.get("id").is_none(){
    if input["jsonrpc"]=="2.0"&&matches!(input["method"].as_str(),Some("notifications/initialized"|"notifications/cancelled")){return Ok(StatusCode::ACCEPTED.into_response());}
    return Err(bad()); // Never execute tools disguised as a notification.
 }
 let core=state.core;let gid=grant.id;
 let result=run_core_blocking("Assistant MCP",move||Ok(crate::assistant_mcp::rpc(&core,&gid,input))).await?;
 let mut response=Json(result).into_response();response.headers_mut().insert("cache-control",HeaderValue::from_static("no-store"));Ok(response)
}
pub(super) async fn grants(State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<Vec<router_core::store::assistant::AssistantGrant>>{authenticated(&headers,&state,false).await?;state.core.store.assistant_grants().map(Json).map_err(core_error)}
pub(super) async fn revoke(Path(gid):Path<String>,State(state):State<MobileHttpState>,headers:HeaderMap)->ApiResult<serde_json::Value>{authenticated(&headers,&state,true).await?;state.core.store.revoke_assistant_grant(&gid).map_err(core_error)?;Ok(Json(serde_json::json!({"revoked":true})))}

#[cfg(test)]
#[path="assistant_http_tests.rs"]
mod tests;
