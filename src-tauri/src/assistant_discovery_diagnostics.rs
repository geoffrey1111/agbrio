//! Bounded listener-local protocol metadata, never request bodies or credentials.
use axum::http::HeaderMap;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::{BTreeMap, VecDeque}, sync::Mutex};

const LIMIT: usize = 64;
fn now() -> i64 { time::OffsetDateTime::now_utc().unix_timestamp_nanos().div_euclid(1_000_000) as i64 }
fn version(raw: Option<&str>) -> &'static str {
    match raw {
        None => "MISSING",
        Some("2026-07-28") => "2026-07-28",
        Some("2025-11-25") => "2025-11-25",
        Some("2025-06-18") => "2025-06-18",
        Some("2025-03-26") => "2025-03-26",
        _ => "OTHER",
    }
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Entry {
    sequence: u64, started_at: i64, finished_at: Option<i64>, method: &'static str,
    header_version: &'static str, requested_version: &'static str,
    accepts_json: bool, accepts_event_stream: bool, authenticated: bool,
    http_status: Option<u16>, rpc_error_code: Option<i64>,
    advertised_events: Option<bool>, event_count: Option<usize>,
}
struct State { entries: VecDeque<Entry>, counts: BTreeMap<&'static str,u64>, sequence: u64 }
pub(super) struct DiscoveryDiagnostics { listener_id: String, started_at: i64, state: Mutex<State> }
impl Default for DiscoveryDiagnostics {
    fn default() -> Self { Self {listener_id:uuid::Uuid::new_v4().to_string(),started_at:now(),state:Mutex::new(State{entries:VecDeque::new(),counts:BTreeMap::new(),sequence:0})} }
}
impl DiscoveryDiagnostics {
    pub(super) fn begin(&self, headers: &HeaderMap, input: &Value) -> Option<u64> {
        let method=match input["method"].as_str() {
            Some("server/discover")=>"server/discover",Some("initialize")=>"initialize",
            Some("events/list")=>"events/list",Some("events/subscribe")=>"events/subscribe",
            Some("events/unsubscribe")=>"events/unsubscribe",Some("tools/list")=>"tools/list",
            Some("tools/call")=>"tools/call",_=>"OTHER",
        };
        let mut s=self.state.lock().ok()?;
        *s.counts.entry(method).or_default()+=1;
        // Tool calls and arbitrary methods cannot evict the discovery evidence.
        if matches!(method,"tools/call"|"OTHER") {return None;}
        s.sequence=s.sequence.saturating_add(1);let sequence=s.sequence;
        let accept=headers.get("accept").and_then(|v|v.to_str().ok()).unwrap_or("");
        if s.entries.len()==LIMIT{s.entries.pop_front();}
        s.entries.push_back(Entry{sequence,started_at:now(),finished_at:None,method,
            header_version:version(headers.get("mcp-protocol-version").and_then(|v|v.to_str().ok())),
            requested_version:version(input.pointer("/params/protocolVersion").and_then(Value::as_str)),
            accepts_json:accept.contains("application/json"),accepts_event_stream:accept.contains("text/event-stream"),
            authenticated:false,http_status:None,rpc_error_code:None,advertised_events:None,event_count:None});
        Some(sequence)
    }
    fn edit(&self,id:Option<u64>,edit:impl FnOnce(&mut Entry)) {
        let Some(id)=id else{return};let Ok(mut s)=self.state.lock()else{return};
        if let Some(e)=s.entries.iter_mut().find(|e|e.sequence==id){edit(e);}
    }
    pub(super) fn authorized(&self,id:Option<u64>){self.edit(id,|e|e.authenticated=true);}
    pub(super) fn rpc(&self,id:Option<u64>,result:&Value){self.edit(id,|e|{
        e.rpc_error_code=result.pointer("/error/code").and_then(Value::as_i64);
        e.advertised_events=result.pointer("/result/capabilities").map(|c|c.get("events").is_some());
        e.event_count=result.pointer("/result/events").and_then(Value::as_array).map(Vec::len);
    });}
    pub(super) fn finish(&self,id:Option<u64>,status:u16){self.edit(id,|e|{e.http_status=Some(status);e.finished_at=Some(now());});}
    pub(super) fn snapshot(&self)->Value {
        let Ok(s)=self.state.lock()else{return json!({"available":false})};
        json!({"listenerId":self.listener_id,"startedAt":self.started_at,"serverVersion":crate::assistant_mcp::SERVER_VERSION,
            "retention":"LAST_64_DISCOVERY_REQUESTS_SINCE_LISTENER_START","requestCounts":s.counts,"recentRequests":s.entries,
            "recordsPrivateContent":false,"platformDiscoveryProven":false})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_bounded_sanitized_and_listener_local() {
        let d=DiscoveryDiagnostics::default();let other=DiscoveryDiagnostics::default();
        let mut headers=HeaderMap::new();headers.insert("authorization","Bearer private-token".parse().unwrap());
        headers.insert("mcp-protocol-version","private-secret".parse().unwrap());
        for _ in 0..80 {let id=d.begin(&headers,&json!({"method":"events/list","params":{"secret":"private-body"}}));d.authorized(id);d.rpc(id,&json!({"result":{"events":[{},{}]}}));d.finish(id,200);}
        for _ in 0..80 {d.begin(&headers,&json!({"method":"private-method"}));d.begin(&headers,&json!({"method":"tools/call","params":{"arguments":{"text":"private-text"}}}));}
        let v=d.snapshot();assert_eq!(v["recentRequests"].as_array().unwrap().len(),64);
        assert_eq!(v["recentRequests"][0]["sequence"],17);assert_eq!(v["recentRequests"][0]["headerVersion"],"OTHER");
        assert_eq!(v["recentRequests"][0]["eventCount"],2);assert_eq!(v["requestCounts"]["tools/call"],80);
        assert!(!v.to_string().contains("private-"));assert!(other.snapshot()["recentRequests"].as_array().unwrap().is_empty());
    }
}
