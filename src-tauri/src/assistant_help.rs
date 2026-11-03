//! Owner-clicked connection help. Desktop IPC only; no OAuth or provider action.
pub(crate) fn url(page:&str)->Result<&'static str,String>{
 match page{
  "CHATGPT_PLUGINS"=>Ok("https://chatgpt.com/plugins"),
  "AUTH_DOCUMENTATION"=>Ok("https://developers.openai.com/plugins/build/auth"),
  "CLAUDE_CONNECTORS"=>Ok("https://claude.ai/customize/connectors"),
  _=>Err("ASSISTANT_HELP_TARGET_INVALID".into()),
 }
}
#[tauri::command]
pub(crate) fn assistant_open_help(page:String)->Result<(),String>{
 let target=url(&page)?;
 crate::platform::open_web_url(target).map_err(|_|"ASSISTANT_HELP_OPEN_FAILED".into())
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn browser_help_uses_only_fixed_public_pages_never_arbitrary_urls_or_oauth_queries(){
  assert_eq!(url("CHATGPT_PLUGINS").unwrap(),"https://chatgpt.com/plugins");
  assert_eq!(url("AUTH_DOCUMENTATION").unwrap(),"https://developers.openai.com/plugins/build/auth");
  for input in ["","https://example.invalid","javascript:alert(1)","file:///private","CHATGPT_PLUGINS?token=private","AUTH_DOCUMENTATION/extra"]{assert!(url(input).is_err());}
 }
}
