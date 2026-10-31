//! Explicit, opt-in paid acceptance. Only isolated task-owned fixtures.
use super::{projection, session::Session, settings, transport::Event};
use router_core::store::pi::Selection;
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) fn image(code: &str, file: &std::path::Path) {
    // Real antialiased font, rather than pixel glyphs that conflate E/F, A/N,
    // or a slashed zero. Python is a fixture renderer only, never Pi transport.
    let source="from PIL import Image,ImageDraw,ImageFont;import sys;i=Image.new('RGB',(640,180),'white');d=ImageDraw.Draw(i);f=ImageFont.truetype('C:/Windows/Fonts/consolab.ttf',96);d.text((32,28),' '.join(sys.argv[1]),font=f,fill='black');i.save(sys.argv[2])";
    assert!(std::process::Command::new("python")
        .args(["-c", source, code])
        .arg(file)
        .status()
        .unwrap()
        .success());
}
#[test]
#[ignore = "paid OpenAI/Anthropic vision+read-tool integration; explicit isolated QA only"]
fn real_two_provider_concurrent_vision_tools_and_restore() {
    assert_eq!(std::env::var("AGBRIO_PI_PAID_QA").as_deref(), Ok("1"));
    let root = PathBuf::from(std::env::var_os("AGBRIO_PI_QA_ROOT").expect("owned QA root"));
    assert!(root.is_absolute() && root.is_dir());
    let workers:Vec<_>=[("openai","gpt-6-luna","png"),("anthropic","claude-haiku-5-5","jpg")].into_iter().map(|(provider,model,extension)|{
        let root=root.clone();std::thread::spawn(move||{
            use base64ct::Encoding;
            let run=uuid::Uuid::new_v4().simple().to_string();let fixture=root.join(format!("paid-{provider}-{run}"));let project=fixture.join("project");let owned=fixture.join("runtime");std::fs::create_dir_all(&project).unwrap();std::fs::create_dir_all(&owned).unwrap();
            let token=format!("PRIVATE_STAGE2_{}",uuid::Uuid::new_v4().simple());let code=uuid::Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase();
            std::fs::write(project.join("fixture.txt"),&token).unwrap();let original=fixture.join(format!("vision.{extension}"));image(&code,&original);
            let selected=Selection{provider:provider.into(),model:model.into(),thinking:"low".into()};let installation=super::installation().unwrap();
            let session=Session::open(&installation,&project,&owned,false,false,Some(&selected),None).unwrap();
            let mime=if extension=="png"{"image/png"}else{"image/jpeg"};let images=vec![json!({"type":"image","mimeType":mime,"data":base64ct::Base64::encode_string(&std::fs::read(&original).unwrap())})];
            let text="Use the read tool to read fixture.txt in your current project. Also read the six-character code in the attached image. Reply only with the exact file content, a vertical bar, and the image code. Do not call any other tool.";
            let accepted=session.submit(&uuid::Uuid::new_v4().simple().to_string(),&"a".repeat(64),text,&images,"SEND").unwrap();assert_eq!(accepted["disposition"],"started");assert!(accepted["nativeEntryId"].is_string());
            let deadline=Instant::now()+Duration::from_secs(180);let mut settled=false;let mut tool=false;
            while Instant::now()<deadline{match session.events.recv_timeout(Duration::from_secs(1)){
                Ok(Event::Record(v))=>{if v["type"]=="tool_execution_end"&&v["toolName"]=="read"&&v["isError"]==false{tool=true;}if v["type"]=="agent_settled"{assert_eq!(v["aborted"],false);settled=true;break;}},
                Ok(Event::Exited)=>panic!("owned Pi exited before settled"),Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{},Err(_)=>panic!("Pi event stream closed"),
            }}
            if !settled{let _=session.cancel();panic!("paid Pi QA did not settle within budget");}
            let history=session.history().unwrap();let entries=history["entries"].as_array().unwrap();assert_eq!(projection::settled_state(entries,history["leafId"].as_str(),false).unwrap(),"RESULT_READY");assert!(tool,"required real read tool receipt");
            let result=settings::call(&session.rpc,json!({"type":"get_last_assistant_text"})).unwrap();let result=result["text"].as_str().unwrap();assert!(result.contains(&token),"file content mismatch");assert!(result.contains(&code),"image-only random code mismatch");
            assert!(session.native_file.is_file());let native_id=session.native_id.clone();let native_file=session.native_file.clone();session.close().unwrap();drop(session);
            let restored=Session::open(&installation,&project,&owned,false,false,Some(&selected),Some((&native_id,&native_file))).unwrap();let previous=settings::call(&restored.rpc,json!({"type":"get_last_assistant_text"})).unwrap();let previous=previous["text"].as_str().unwrap();assert!(previous.contains(&token)&&previous.contains(&code));restored.close().unwrap();
            eprintln!("{provider}/{model}/low: independent native session, real read+{extension} vision, result and exact original restore PASS");
        })
    }).collect();
    let results: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    assert!(
        results.iter().all(Result::is_ok),
        "one or more provider acceptance gates failed"
    );
}
