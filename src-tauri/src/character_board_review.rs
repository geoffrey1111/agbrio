//! Explicit owner UI review proxy. Not MCP; no general URL, headers, storage or retry.
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
const ORIGIN:&str="http://127.0.0.1:47816";
const LIMIT:usize=64*1024;
#[derive(Deserialize)]
pub(crate) struct ReviewInput{pub method:String,pub route:String,#[serde(default)]pub body:Value,pub token:Option<String>}
#[derive(Serialize)]
pub(crate) struct ReviewResponse{pub status:u16,pub json:Value}
fn error(status:u16,message:&str)->ReviewResponse{ReviewResponse{status,json:json!({"error":message})}}
fn allowed(input:&ReviewInput)->bool{
 match (input.method.as_str(),input.route.as_str()){
  ("GET","/api/reviews")=>input.body.is_null(),
  ("POST","/api/draft"|"/api/submit")=>true,
  ("POST",route)=>route.strip_prefix("/api/notify/").is_some_and(|id|id.len()==36&&uuid::Uuid::parse_str(id).is_ok()),
  _=>false
 }
}
pub(crate) async fn proxy(revision:String,input:ReviewInput)->ReviewResponse{
 if !allowed(&input){return error(403,"BOARD_REVIEW_ROUTE_BLOCKED")}
 let revision_valid=tauri::async_runtime::spawn_blocking(move||crate::character_board::snapshot(Some(&revision)).is_ok_and(|s|s.revision==revision)).await.unwrap_or(false);
 if !revision_valid{return error(409,"看板已更新，请刷新后重新核对候选图。")}
 let origin=ORIGIN.to_string();
 #[cfg(test)] let origin=std::env::var("AGBRIO_BOARD_REVIEW_TEST_ORIGIN").unwrap_or(origin);
 forward(&origin,input).await
}
async fn forward(origin:&str,input:ReviewInput)->ReviewResponse{
 if !allowed(&input){return error(403,"BOARD_REVIEW_ROUTE_BLOCKED")}
 let Ok(body)=serde_json::to_vec(&input.body)else{return error(400,"BOARD_REVIEW_INVALID_BODY")};
 if body.len()>LIMIT||input.token.as_ref().is_some_and(|t|t.len()>256||t.contains(['\r','\n'])){return error(413,"BOARD_REVIEW_BODY_TOO_LARGE")}
 let Ok(client)=reqwest::Client::builder().no_proxy().retry(reqwest::retry::never()).redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(18)).build()else{return error(503,"BOARD_REVIEW_UNAVAILABLE")};
 let mut request=if input.method=="GET"{client.get(format!("{origin}{}",input.route))}else{client.post(format!("{origin}{}",input.route)).body(body)};
 request=request.header("Origin",origin).header("Content-Type","application/json");if let Some(token)=input.token{request=request.header("X-Board-Token",token);}
 let mut response=match request.send().await{Ok(r)=>r,Err(e)=>return if e.is_connect(){error(503,"电脑上的批复服务未启动，请运行 启动看板.ps1")}else{error(504,"批复回执未确认；请核对已保存状态，不要重复提交。")}};
 let status=response.status().as_u16();let mut bytes=Vec::new();
 loop{match response.chunk().await{Ok(Some(chunk))=>{if bytes.len()+chunk.len()>8*1024*1024{return error(502,"BOARD_REVIEW_RESPONSE_TOO_LARGE")};bytes.extend_from_slice(&chunk)},Ok(None)=>break,Err(_)=>return error(502,"批复回执读取中断；请核对已保存状态。")}}
 match serde_json::from_slice(&bytes){Ok(json)=>ReviewResponse{status,json},Err(_)=>error(502,"BOARD_REVIEW_INVALID_RESPONSE")}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn allowlist_refuses_other_methods_encoded_paths_queries_and_origins(){
  for (method,route) in [("GET","/api/reviews?all=1"),("GET","http://127.0.0.1:47816/api/reviews"),("POST","/api/reviews"),("DELETE","/api/draft"),("POST","/api/notify/../../draft"),("POST","/api/notify/00000000000000000000000000000000"),("POST","/api/%73ubmit")]{assert!(!allowed(&ReviewInput{method:method.into(),route:route.into(),body:Value::Null,token:None}));}
  for (method,route) in [("GET","/api/reviews"),("POST","/api/draft"),("POST","/api/submit"),("POST","/api/notify/00000000-0000-4000-8000-000000000000")]{assert!(allowed(&ReviewInput{method:method.into(),route:route.into(),body:Value::Null,token:None}));}
 }
 #[test]fn a_lost_write_reply_is_not_retried_and_a_closed_service_has_the_owner_message(){
  use std::io::Read;
  let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());
  let server=std::thread::spawn(move||{let(mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();let mut bytes=[0;4096];assert!(stream.read(&mut bytes).unwrap()>0);drop(stream);listener.set_nonblocking(true).unwrap();let deadline=std::time::Instant::now()+std::time::Duration::from_millis(200);let mut calls=1;while std::time::Instant::now()<deadline{if listener.accept().is_ok(){calls+=1;}std::thread::sleep(std::time::Duration::from_millis(10));}calls});
  let runtime=tokio::runtime::Runtime::new().unwrap();let result=runtime.block_on(forward(&origin,ReviewInput{method:"POST".into(),route:"/api/draft".into(),body:json!({"fixture":true}),token:Some("fixture-only".into())}));
  assert_eq!(result.status,504);assert_eq!(server.join().unwrap(),1);
  let closed=runtime.block_on(forward(&origin,ReviewInput{method:"GET".into(),route:"/api/reviews".into(),body:Value::Null,token:None}));assert_eq!(closed.status,503);assert_eq!(closed.json["error"],"电脑上的批复服务未启动，请运行 启动看板.ps1");
 }
}
