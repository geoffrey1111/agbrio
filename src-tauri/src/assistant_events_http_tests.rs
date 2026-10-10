//! Controlled HTTPS peer. Only the cfg(test) connector pins localhost; the
//! shipping connector still rejects it. These are protocol proofs, not Dot UAT.
use super::*;
use crate::assistant_events::{dispatch_one, TestCallbackClient};
use base64ct::{Base64, Encoding};
use router_core::store::mcp_events::{DECISION_REQUIRED, REPLY_READY};
use std::os::windows::process::CommandExt;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};

struct Callback {
    child: Child,
    root: PathBuf,
    port: u16,
    client: reqwest::Client,
}
impl Drop for Callback {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Callback {
    async fn start(root: &Path) -> Self {
        std::fs::create_dir_all(root).unwrap();
        let cert = root.join("cert.pem");
        let key = root.join("key.pem");
        let script = r#"$ErrorActionPreference='Stop'; $rsa=[System.Security.Cryptography.RSA]::Create(2048); $req=[System.Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=localhost',$rsa,[System.Security.Cryptography.HashAlgorithmName]::SHA256,[System.Security.Cryptography.RSASignaturePadding]::Pkcs1); $san=[System.Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new(); $san.AddDnsName('localhost'); $req.CertificateExtensions.Add($san.Build()); $req.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($false,$false,0,$true)); $cert=$req.CreateSelfSigned([DateTimeOffset]::UtcNow.AddMinutes(-5),[DateTimeOffset]::UtcNow.AddHours(1)); [IO.File]::WriteAllText($env:EVENT_TEST_CERT,$cert.ExportCertificatePem()); [IO.File]::WriteAllText($env:EVENT_TEST_KEY,$rsa.ExportPkcs8PrivateKeyPem()); $cert.Dispose(); $rsa.Dispose();"#;
        assert!(Command::new("pwsh")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-Command", script])
            .env("EVENT_TEST_CERT", &cert)
            .env("EVENT_TEST_KEY", &key)
            .status()
            .unwrap()
            .success());
        let path = root.join("receiver.mjs");
        std::fs::write(&path,r#"
import https from 'node:https';import fs from 'node:fs';import crypto from 'node:crypto';
const root=process.argv[2];let events=0;
const server=https.createServer({key:fs.readFileSync(root+'/key.pem'),cert:fs.readFileSync(root+'/cert.pem')},(req,res)=>{
 const chunks=[];req.on('data',c=>chunks.push(c));req.on('end',()=>{
  const raw=Buffer.concat(chunks),id=req.headers['webhook-id'],at=req.headers['webhook-timestamp'];
  const signatures=(req.headers['webhook-signature']??'').split(' ');
  const matches=[1,2].filter(n=>signatures.includes('v1,'+crypto.createHmac('sha256',Buffer.alloc(32,n)).update(id+'.'+at+'.').update(raw).digest('base64')));
  const body=JSON.parse(raw);const valid=matches.length>0&&!!req.headers['x-mcp-subscription-id']&&Math.abs(Date.now()/1000-Number(at))<60;
  fs.appendFileSync(root+'/received.jsonl',JSON.stringify({id,at,body,matches,valid,path:req.url,subscription:req.headers['x-mcp-subscription-id']})+'\n');
  if(!valid){res.writeHead(401);res.end();return;}
  if(req.url==='/redirect'){res.writeHead(307,{Location:'https://localhost:1/untrusted'});res.end();return;}
  if(body.type==='verification'){
   res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({challenge:req.url==='/wrong'?'not-the-challenge':body.challenge}));return;
  }
  events++;res.writeHead(req.url==='/retry'&&events===1?503:204);res.end();
 });
});server.listen(0,'127.0.0.1',()=>fs.writeFileSync(root+'/port',String(server.address().port)));
"#).unwrap();
        let child = Command::new("node")
            .creation_flags(0x08000000)
            .arg(path)
            .arg(root)
            .spawn()
            .unwrap();
        let port_path = root.join("port");
        let start = Instant::now();
        while !port_path.exists() {
            assert!(start.elapsed() < Duration::from_secs(5));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let port = std::fs::read_to_string(port_path)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .add_root_certificate(
                reqwest::Certificate::from_pem(&std::fs::read(cert).unwrap()).unwrap(),
            )
            .resolve("localhost", ([127, 0, 0, 1], port).into())
            .build()
            .unwrap();
        Self {
            child,
            root: root.into(),
            port,
            client,
        }
    }
    fn url(&self, path: &str) -> String {
        format!("https://localhost:{}{path}", self.port)
    }
    fn attach(&self, gid: &str, path: &str) -> TestCallbackClient {
        TestCallbackClient::install(gid, &self.url(path), self.client.clone())
    }
    fn records(&self) -> Vec<Value> {
        std::fs::read_to_string(self.root.join("received.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}
async fn event_rpc(
    c: &reqwest::Client,
    base: &str,
    token: &str,
    method: &str,
    params: Value,
) -> Value {
    let response = c
        .post(format!("{base}/mcp"))
        .header("host", "assistant.fixture.invalid")
        .header("origin", "https://assistant.fixture.invalid")
        .header("accept", "application/json")
        .header("mcp-protocol-version", "2026-07-28")
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","id":"contract","method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    response.json().await.unwrap()
}
fn input(core: &RouterCore, gid: &str, url: &str, name: &str, key: u8) -> Value {
    let g = core.store.assistant_grant(gid).unwrap();
    json!({"name":name,"arguments":{"workstreamId":g.workstream_id,"bindingRevision":g.binding_revision,"sourceRole":"DECISION"},"delivery":{"mode":"webhook","url":url,"secret":format!("whsec_{}",Base64::encode_string(&[key;32]))},"ttlMs":600000})
}
fn emit(core: &RouterCore, gid: &str, identity: &str) -> String {
    let g = core.store.assistant_grant(gid).unwrap();
    let ep = core
        .store
        .role_bridge(&g.workstream_id)
        .unwrap()
        .decision
        .unwrap()
        .endpoint;
    core.store
        .record_reply_observation(
            &g.workstream_id,
            &ep.id,
            Some(identity),
            "Exact complete disposable source for the authorized handoff.",
            None,
        )
        .unwrap();
    core.store
        .note_reply_completion(&g.workstream_id, &ep.id, identity, Some(now_ms()))
        .unwrap();
    core.store
        .reply_observations_for_workstream(&g.workstream_id)
        .unwrap()
        .into_iter()
        .find(|o| o.assistant_identity.as_deref() == Some(identity))
        .unwrap()
        .id
}
fn sql(dir: &Path, q: &str) {
    rusqlite::Connection::open(dir.join("router.db"))
        .unwrap()
        .execute_batch(q)
        .unwrap();
}

#[test]
fn authenticated_mcp_events_discovery_challenge_retry_restart_rotation_and_single_native_send() {
    let (d, core, gid, _) = fixture();
    let host = crate::HostRuntime::default();
    tokio::runtime::Runtime::new().unwrap().block_on(async{
  let peer=Callback::start(&d.path().join("tls-peer")).await;let _pin=peer.attach(&gid,"/retry");
  let handle=start(core.clone(),config(d.path()),host.clone()).await.unwrap();let base=format!("http://{}",handle.address);let c=client();let token=core.store.issue_assistant_access(&gid,"https://assistant.fixture.invalid/mcp",now_ms()+600000).unwrap();
  let discovered=event_rpc(&c,&base,&token,"server/discover",json!({})).await;assert_eq!(discovered["result"]["resultType"],"complete");assert_eq!(discovered["result"]["supportedVersions"],json!(["2026-07-28"]));assert!(discovered["result"]["capabilities"]["events"].is_object());
  let catalog=event_rpc(&c,&base,&token,"events/list",json!({})).await;assert_eq!(catalog["result"]["events"].as_array().unwrap().len(),2);assert_eq!(catalog["result"]["events"][0]["inputSchema"]["properties"]["sourceRole"]["enum"],json!(["DECISION"]));
  assert_eq!(event_rpc(&c,&base,&token,"events/list",json!({"cursor":"unsupported"})).await["error"]["code"],-32602);
  assert_eq!(event_rpc(&c,&base,&token,"events/unknown",json!({})).await["error"]["code"],-32601);
  let args=input(&core,&gid,&peer.url("/retry"),REPLY_READY,1);let subscribed=event_rpc(&c,&base,&token,"events/subscribe",args.clone()).await;assert!(subscribed["result"]["id"].is_string(),"{subscribed}");let sid=subscribed["result"]["id"].as_str().unwrap().to_string();
  let repeat=event_rpc(&c,&base,&token,"events/subscribe",args.clone()).await;assert_eq!(repeat["result"]["id"],sid);assert_eq!(peer.records().len(),1,"same callback/key must reuse verification");
  let observation=emit(&core,&gid,"codex:event-transport:item");dispatch_one(&core).await;
  assert_eq!(peer.records().len(),2);assert_eq!(core.store.event_status_for_grant(&gid).unwrap()["webhookReceipts"],0);
  let first=peer.records()[1].clone();assert!(first["valid"].as_bool().unwrap());assert_eq!(first["id"],first["body"]["eventId"]);assert_eq!(first["body"]["data"]["observationId"],observation);assert_eq!(first["body"]["data"]["sourceKind"],"REPLY");assert!(first["body"].to_string().find("Exact complete").is_none());
  // The store is reconstructed after a failed HTTP delivery, not a new event.
  tokio::time::sleep(Duration::from_millis(1100)).await;
  let recovered=RouterCore{store:Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap()),..core.clone()};sql(d.path(),"UPDATE mcp_event_deliveries SET next_attempt_at=0 WHERE status='QUEUED'");
  dispatch_one(&recovered).await;let second=peer.records()[2].clone();assert_eq!(second["body"],first["body"]);assert_eq!(second["id"],first["id"]);assert!(second["at"].as_str().unwrap().parse::<i64>().unwrap()>first["at"].as_str().unwrap().parse::<i64>().unwrap());assert_eq!(core.store.event_status_for_grant(&gid).unwrap()["webhookReceipts"],1);dispatch_one(&core).await;assert_eq!(peer.records().len(),3);
  let source=rpc(&c,&base,&token,"agbrio_read_source",json!({"observationId":observation})).await;assert!(!source["result"]["isError"].as_bool().unwrap());assert!(source.to_string().contains("Exact complete disposable"));
  let prepare=json!({"eventId":first["id"],"requestId":"delivery-one","observationId":observation,"text":"Reviewed disposable instruction: acknowledge once.","attachmentIds":[]});
  let prepared=rpc(&c,&base,&token,"agbrio_prepare_handoff",prepare.clone()).await;assert_eq!(prepared["result"]["isError"],false,"{prepared}");let h=&prepared["result"]["structuredContent"];
  let mut duplicate=prepare;duplicate["requestId"]=json!("delivery-retry-new-run");let again=rpc(&c,&base,&token,"agbrio_prepare_handoff",duplicate).await;assert_eq!(again["result"]["structuredContent"]["id"],h["id"]);
  let send=json!({"handoffId":h["id"],"expectedHash":h["payloadHash"],"ruleId":"continue","decisionId":null,"assessment":"Review of exact disposable payload under owner-authorized rule"});let sent=rpc(&c,&base,&token,"agbrio_confirm_and_send",send.clone()).await;assert_eq!(sent["result"]["structuredContent"]["status"],"SENT","{sent}");
  let duplicate_send=rpc(&c,&base,&token,"agbrio_confirm_and_send",send).await;assert_eq!(duplicate_send["result"]["isError"],true);let receipt=rpc(&c,&base,&token,"agbrio_receipt",json!({"handoffId":h["id"],"expectedHash":h["payloadHash"]})).await;assert_eq!(receipt["result"]["structuredContent"]["handoff"]["status"],"SENT");assert_eq!(std::fs::read_to_string(d.path().join("native-writes.jsonl")).unwrap().lines().count(),1);
  let mut rotated=args.clone();rotated["delivery"]["secret"]=json!(format!("whsec_{}",Base64::encode_string(&[2u8;32])));assert_eq!(event_rpc(&c,&base,&token,"events/subscribe",rotated).await["result"]["id"],sid);emit(&core,&gid,"codex:event-rotation:item");dispatch_one(&core).await;let rec=peer.records();let last=rec.last().unwrap();assert_eq!(last["matches"],json!([1,2]));
  let mut unsub=args;unsub["delivery"].as_object_mut().unwrap().remove("secret");assert_eq!(event_rpc(&c,&base,&token,"events/unsubscribe",unsub.clone()).await["result"],json!({}));assert_eq!(event_rpc(&c,&base,&token,"events/unsubscribe",unsub).await["result"],json!({}));let size=peer.records().len();emit(&core,&gid,"codex:event-after-unsubscribe:item");dispatch_one(&core).await;assert_eq!(peer.records().len(),size);assert!(core.store.event_subscription(&sid).unwrap().unwrap().secret.is_empty());
  handle.shutdown.send(()).unwrap();handle.task.await.unwrap();
 });
}

#[test]
fn real_http_event_negative_controls_and_native_question_resolution() {
    let (d, core, gid, _) = fixture();
    let host = crate::HostRuntime::default();
    tokio::runtime::Runtime::new().unwrap().block_on(async{
  let peer=Callback::start(&d.path().join("negative-tls-peer")).await;let _a=peer.attach(&gid,"/wrong");let _b=peer.attach(&gid,"/redirect");let _c=peer.attach(&gid,"/question");
  let handle=start(core.clone(),config(d.path()),host.clone()).await.unwrap();let base=format!("http://{}",handle.address);let c=client();let token=core.store.issue_assistant_access(&gid,"https://assistant.fixture.invalid/mcp",now_ms()+600000).unwrap();
  for path in ["/wrong","/redirect"]{let r=event_rpc(&c,&base,&token,"events/subscribe",input(&core,&gid,&peer.url(path),REPLY_READY,1)).await;assert_eq!(r["error"]["code"],-32015,"{r}");}assert_eq!(core.store.event_status_for_grant(&gid).unwrap()["activeSubscriptions"],0);
  let blocked=event_rpc(&c,&base,&token,"events/subscribe",input(&core,&gid,"https://127.0.0.1/private",REPLY_READY,1)).await;assert_eq!(blocked["error"]["code"],-32015);
  // No test connector is registered for this URL: shipping DNS policy rejects localhost.
  let dns_blocked=event_rpc(&c,&base,&token,"events/subscribe",input(&core,&gid,&peer.url("/production-policy"),REPLY_READY,1)).await;assert_eq!(dns_blocked["error"]["code"],-32015);
  let mut forbidden=input(&core,&gid,&peer.url("/question"),REPLY_READY,1);forbidden["arguments"]["sourceRole"]=json!("EXECUTION");assert_eq!(event_rpc(&c,&base,&token,"events/subscribe",forbidden).await["error"]["code"],-32024);
  let args=input(&core,&gid,&peer.url("/question"),DECISION_REQUIRED,1);let r=event_rpc(&c,&base,&token,"events/subscribe",args).await;assert!(r["result"]["id"].is_string(),"{r}");
  let frame=json!({"jsonrpc":"2.0","id":100,"method":"item/tool/requestUserInput","params":{"threadId":"qa-source-thread","turnId":"question-turn","itemId":"fixture-question","questions":[{"id":"option","header":"Fixture choice","question":"Choose one disposable fixture option","options":[]}]}});
  crate::host_application::capture_pending_codex_request(&core.session,&frame);crate::host_application::publish_mcp_request_state(&core.store,&core.session,&frame);dispatch_one(&core).await;
  let record=peer.records().last().unwrap().clone();assert_eq!(record["body"]["name"],DECISION_REQUIRED);assert_eq!(record["body"]["data"]["sourceKind"],"NATIVE_REQUEST");let source=rpc(&c,&base,&token,"agbrio_read_source",json!({"observationId":record["body"]["data"]["observationId"]})).await;assert_eq!(source["result"]["structuredContent"]["canPrepareHandoff"],false);assert_eq!(source["result"]["structuredContent"]["pendingConfirmed"],true);assert!(source.to_string().contains("Choose one disposable fixture"));
  core.store.observe_mcp_native_request("qa-source-thread","question-turn","101",&json!({"id":"resolved-question"})).unwrap();core.store.resolve_mcp_native_request("qa-source-thread",Some("question-turn"),None).unwrap();let size=peer.records().len();dispatch_one(&core).await;assert_eq!(peer.records().len(),size);
  core.store.revoke_assistant_grant(&gid).unwrap();let unauth=c.post(format!("{base}/mcp")).header("host","assistant.fixture.invalid").header("accept","application/json").header("mcp-protocol-version","2026-07-28").bearer_auth(&token).json(&json!({"jsonrpc":"2.0","id":1,"method":"events/list"})).send().await.unwrap();assert_eq!(unauth.status(),StatusCode::UNAUTHORIZED);
  handle.shutdown.send(()).unwrap();handle.task.await.unwrap();
 });
}

#[test]
fn resident_worker_delivers_without_ui_navigation_or_consumer_polling() {
    let (d, core, gid, _) = fixture();
    let host = crate::HostRuntime::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let peer = Callback::start(&d.path().join("resident-tls-peer")).await;
        let _pin = peer.attach(&gid, "/resident");
        let handle = start(core.clone(), config(d.path()), host.clone())
            .await
            .unwrap();
        let base = format!("http://{}", handle.address);
        let c = client();
        let token = core
            .store
            .issue_assistant_access(
                &gid,
                "https://assistant.fixture.invalid/mcp",
                now_ms() + 600000,
            )
            .unwrap();
        assert!(event_rpc(
            &c,
            &base,
            &token,
            "events/subscribe",
            input(&core, &gid, &peer.url("/resident"), REPLY_READY, 1)
        )
        .await["result"]["id"]
            .is_string());
        host.start_event_delivery(core.clone());
        emit(&core, &gid, "codex:resident-with-no-reader:item");
        // Only inspect the receiver's local evidence, never read/poll a source tool.
        let start = Instant::now();
        while peer.records().len() < 2 {
            assert!(start.elapsed() < Duration::from_secs(5));
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(peer.records()[1]["body"]["name"], REPLY_READY);
        handle.shutdown.send(()).unwrap();
        handle.task.await.unwrap();
    });
    host.shutdown(&core);
}
