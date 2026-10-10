//! Callback-only correction when OS DNS returns benchmark-range peers.
//! No global resolver changes, private-IP allowance, URLs/keys or native error logs.
use super::{public_ip,validated_dns_peers};
use hickory_proto::{op::{Message,MessageType,OpCode,Query,ResponseCode},rr::{DNSClass,Name,RData,RecordType}};
use std::{net::{IpAddr,SocketAddr},time::Duration};
const FAILED:&str="MCP_EVENT_CALLBACK_BENCHMARK_PUBLIC_DNS_FAILED";
const INVALID:&str="MCP_EVENT_CALLBACK_BENCHMARK_PUBLIC_DNS_INVALID";
const TIMEOUT:&str="MCP_EVENT_CALLBACK_BENCHMARK_PUBLIC_DNS_TIMEOUT";
const ENDPOINT:&str="https://cloudflare-dns.com/dns-query";
const MAX_BODY:usize=16384;

fn query(host:&str,kind:RecordType)->Result<Message,String> {
    let mut name=Name::from_ascii(host).map_err(|_|INVALID)?.to_lowercase();
    name.set_fqdn(true);
    let id=u16::from_be_bytes(uuid::Uuid::new_v4().as_bytes()[..2].try_into().unwrap());
    let mut message=Message::new(id,MessageType::Query,OpCode::Query);
    message.metadata.recursion_desired=true;
    message.add_query(Query::query(name,kind));
    Ok(message)
}
fn response_addresses(query:&Message,bytes:&[u8],port:u16)->Result<Vec<SocketAddr>,String> {
    if bytes.len()>MAX_BODY {return Err(INVALID.into());}
    let reply=Message::from_vec(bytes).map_err(|_|INVALID)?;
    if reply.metadata.id!=query.metadata.id || reply.metadata.message_type!=MessageType::Response
        || reply.metadata.op_code!=OpCode::Query || reply.metadata.truncation
        || reply.metadata.response_code!=ResponseCode::NoError || reply.queries!=query.queries
        || reply.answers.len()>64 {return Err(INVALID.into());}
    let requested=&query.queries[0];
    let mut names=vec![requested.name().to_lowercase()];
    // A bounded CNAME chain is allowed; unrelated owners and conflicting aliases
    // must not turn a different question's address into this callback's route.
    for _ in 0..8 {
        let current=names.last().unwrap();
        let aliases=reply.answers.iter().filter_map(|r|match &r.data{
            RData::CNAME(c) if r.name.to_lowercase()==*current && r.dns_class==DNSClass::IN => Some(c.0.to_lowercase()),
            _=>None,
        }).collect::<Vec<_>>();
        if aliases.is_empty(){break;}
        if aliases.len()!=1 || names.contains(&aliases[0]) {return Err(INVALID.into());}
        names.push(aliases[0].clone());
    }
    if reply.answers.iter().any(|r|matches!(&r.data,RData::CNAME(_)) && r.name.to_lowercase()==*names.last().unwrap()) {return Err(INVALID.into());}
    let mut peers=vec![];
    for r in &reply.answers {
        let ip=match &r.data{RData::A(a)=>Some(IpAddr::V4(a.0)),RData::AAAA(a)=>Some(IpAddr::V6(a.0)),_=>None};
        if let Some(ip)=ip {
            if r.dns_class!=DNSClass::IN || !names.contains(&r.name.to_lowercase())
                || r.record_type()!=requested.query_type() {return Err(INVALID.into());}
            peers.push(SocketAddr::new(ip,port));
        }
    }
    Ok(peers)
}
fn pinned_client()->Result<reqwest::Client,String> {
    let peers=[SocketAddr::from(([1,1,1,1],443)),SocketAddr::from(([1,0,0,1],443))];
    if peers.iter().any(|p|!public_ip(p.ip())){return Err(FAILED.into());}
    reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(4)).resolve_to_addrs("cloudflare-dns.com",&peers)
        .build().map_err(|_|FAILED.into())
}
async fn exchange(client:&reqwest::Client,query:Message,port:u16)->Result<Vec<SocketAddr>,String> {
    exchange_at(client,ENDPOINT,query,port).await
}
async fn exchange_at(client:&reqwest::Client,endpoint:&str,query:Message,port:u16)->Result<Vec<SocketAddr>,String> {
    let bytes=query.to_vec().map_err(|_|INVALID)?;
    let mut response=client.post(endpoint).header("accept","application/dns-message")
        .header("content-type","application/dns-message").body(bytes).send().await
        .map_err(|e|if e.is_timeout(){TIMEOUT}else{FAILED})?;
    if response.status()!=reqwest::StatusCode::OK || response.headers().get("content-type")
        .and_then(|v|v.to_str().ok()).is_none_or(|v|v.split(';').next().unwrap_or("").trim()!="application/dns-message") {
        return Err(FAILED.into());
    }
    let mut body=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_|FAILED)? {
        if body.len()+chunk.len()>MAX_BODY{return Err(INVALID.into());}
        body.extend_from_slice(&chunk);
    }
    response_addresses(&query,&body,port)
}
pub(super) async fn resolve_benchmark_domain(u:&url::Url)->Result<Vec<SocketAddr>,String> {
    let url::Host::Domain(host)=u.host().ok_or(INVALID)? else{return Err(INVALID.into());};
    let port=u.port_or_known_default().ok_or(INVALID)?;
    let client=pinned_client()?;
    let a=query(host,RecordType::A)?;let aaaa=query(host,RecordType::AAAA)?;
    let peers=tokio::time::timeout(Duration::from_secs(4),async {
        let (a,aaaa)=tokio::join!(exchange(&client,a,port),exchange(&client,aaaa,port));
        let mut peers=a?;peers.extend(aaaa?);Ok::<_,String>(peers)
    }).await.map_err(|_|TIMEOUT)??;
    validated_dns_peers(peers).map_err(|e|if e=="MCP_EVENT_CALLBACK_DNS_EMPTY"{
        "MCP_EVENT_CALLBACK_BENCHMARK_PUBLIC_DNS_EMPTY".into()
    }else{"MCP_EVENT_CALLBACK_BENCHMARK_PUBLIC_DNS_NONPUBLIC".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::{Record,rdata::{A,CNAME}};
    fn reply(q:&Message,owner:&str,address:[u8;4])->Message {
        let mut r=Message::new(q.metadata.id,MessageType::Response,OpCode::Query);
        r.queries=q.queries.clone();
        r.answers.push(Record::from_rdata(Name::from_ascii(owner).unwrap(),60,RData::A(A(address.into()))));r
    }
    #[test]
    fn wire_response_requires_exact_question_and_bounded_alias_chain() {
        let q=query("callback.fixture.invalid",RecordType::A).unwrap();
        let valid=reply(&q,"callback.fixture.invalid",[1,1,1,1]);
        assert_eq!(response_addresses(&q,&valid.to_vec().unwrap(),443).unwrap().len(),1);
        let mut bad=valid.clone();bad.metadata.id^=1;assert!(response_addresses(&q,&bad.to_vec().unwrap(),443).is_err());
        bad=valid.clone();bad.queries=query("other.fixture.invalid",RecordType::A).unwrap().queries;assert!(response_addresses(&q,&bad.to_vec().unwrap(),443).is_err());
        bad=valid.clone();bad.metadata.truncation=true;assert!(response_addresses(&q,&bad.to_vec().unwrap(),443).is_err());
        bad=reply(&q,"unrelated.fixture.invalid",[1,1,1,1]);assert!(response_addresses(&q,&bad.to_vec().unwrap(),443).is_err());
        assert!(response_addresses(&q,&vec![0;MAX_BODY+1],443).is_err());
        assert!(response_addresses(&q,&[0,1,2],443).is_err());
        let mut alias=reply(&q,"alias.fixture.invalid",[1,1,1,1]);
        alias.answers.push(Record::from_rdata(q.queries[0].name().clone(),60,RData::CNAME(CNAME(Name::from_ascii("alias.fixture.invalid").unwrap()))));
        assert_eq!(response_addresses(&q,&alias.to_vec().unwrap(),443).unwrap().len(),1);
        alias.answers.push(Record::from_rdata(Name::from_ascii("alias.fixture.invalid").unwrap(),60,RData::CNAME(CNAME(q.queries[0].name().clone()))));
        assert!(response_addresses(&q,&alias.to_vec().unwrap(),443).is_err());
    }
    #[test]
    fn public_resolver_answers_still_require_every_peer_to_be_public() {
        let q=query("callback.fixture.invalid",RecordType::A).unwrap();
        let mut r=reply(&q,"callback.fixture.invalid",[1,1,1,1]);
        r.answers.push(Record::from_rdata(q.queries[0].name().clone(),60,RData::A(A([10,0,0,1].into()))));
        let peers=response_addresses(&q,&r.to_vec().unwrap(),443).unwrap();
        assert_eq!(validated_dns_peers(peers).unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_ADDRESS");
    }
    #[test]
    fn doh_transport_rejects_redirect_wrong_media_oversize_and_truncated_wire() {
        let runtime=tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            use axum::{routing::post,Router,body::Bytes};
            use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
            let followups=Arc::new(AtomicUsize::new(0));let hits=followups.clone();
            let app=Router::new()
                .route("/redirect",post(||async{(axum::http::StatusCode::TEMPORARY_REDIRECT,[("location","/followup")],"")}))
                .route("/followup",post(move||{let hits=hits.clone();async move {hits.fetch_add(1,Ordering::SeqCst);"must-not-be-reached"}}))
                .route("/media",post(||async{([( "content-type","application/json")],"{\"private\":true}")}))
                .route("/large",post(||async{([( "content-type","application/dns-message")],vec![0u8;MAX_BODY+1])}))
                .route("/broken",post(||async{([( "content-type","application/dns-message")],vec![0u8;3])}))
                .route("/valid",post(|bytes:Bytes|async move{
                    let q=Message::from_vec(&bytes).unwrap();
                    let r=reply(&q,"callback.fixture.invalid",[1,1,1,1]);
                    ([("content-type","application/dns-message")],r.to_vec().unwrap())
                }));
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address=listener.local_addr().unwrap();
            let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});
            // Fixture transport only; shipping uses pinned_client+constant HTTPS
            // endpoint, never this client or the localhost fixture address.
            let client=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().unwrap();
            for (path,error) in [("redirect",FAILED),("media",FAILED),("large",INVALID),("broken",INVALID)]{
                let q=query("callback.fixture.invalid",RecordType::A).unwrap();
                assert_eq!(exchange_at(&client,&format!("http://{address}/{path}"),q,443).await.unwrap_err(),error);
            }
            assert_eq!(followups.load(Ordering::SeqCst),0);
            assert_eq!(exchange_at(&client,&format!("http://{address}/valid"),query("callback.fixture.invalid",RecordType::A).unwrap(),443).await.unwrap().len(),1);
            server.abort();let _=server.await;
        });
    }
    #[test]
    #[ignore="Explicit pinned public DoH of two fixed public domains; no callback HTTP, subscription or inference"]
    fn actual_benchmark_callback_correction_dns_only() {
        let runtime=tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            for (index,host) in ["chatgpt.com","api.openai.com"].into_iter().enumerate(){
                let u=super::super::callback_url(&format!("https://{host}/")).unwrap();
                let before=super::super::callback_peers(&u).await;
                assert_eq!(before.unwrap_err(),"MCP_EVENT_CALLBACK_DNS_NONPUBLIC_BENCHMARK");
                let after=super::super::callback_peers_for_connect(&u).await.unwrap();
                assert!(!after.is_empty()&&after.iter().all(|p|public_ip(p.ip())));
                println!("{}",serde_json::json!({"probe":index+1,"initialClass":"BENCHMARK_RANGE","correctedClass":"ALL_PUBLIC","resolverPath":"OS_SYSTEM_THEN_PINNED_PUBLIC_DOH","callbackHttpRequested":false}));
            }
        });
    }
}
