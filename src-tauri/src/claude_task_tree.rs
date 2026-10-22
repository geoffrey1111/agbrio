//! Read-only desktop task relationships. No plugin/model calls or store writes.
use serde::{Deserialize, Serialize, de::{self, MapAccess, Visitor}, Deserializer};
use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, fs::File, io::{Read, BufReader}, sync::{Mutex, OnceLock}, time::SystemTime};
#[cfg(not(test))] use std::time::{Instant,Duration};

#[derive(Default, Clone, Deserialize)]
#[serde(rename_all="camelCase")]
struct Spawn { session_id: String }
#[derive(Default, Clone)]
struct Meta { local:String, native:String, cwd:String, title:String, parent:Option<Spawn>, archived:bool, incomplete:bool }
impl<'de> Visitor<'de> for &mut Meta {
    type Value=();
    fn expecting(&self,f:&mut std::fmt::Formatter)->std::fmt::Result{f.write_str("whitelisted task relationship metadata")}
    fn visit_map<A:MapAccess<'de>>(self,mut map:A)->Result<(),A::Error>{
        while let Some(k)=map.next_key::<String>()? {
            match k.as_str(){
                "sessionId"=>self.local=map.next_value()?, "cliSessionId"=>self.native=map.next_value()?,
                "cwd"=>self.cwd=map.next_value()?, "title"=>self.title=map.next_value::<Option<String>>()?.unwrap_or_default(),
                "isArchived"=>self.archived=map.next_value::<Option<bool>>()?.unwrap_or(false),
                "spawnedFrom"=>{self.parent=map.next_value()?;return Err(de::Error::custom("RELATION_COMPLETE"));},
                // Payload values before spawnedFrom are streamed past, never retained.
                "messages"|"transcript"|"spawnSeed"=>return Err(de::Error::custom("RELATION_ENDED")),
                _=>{map.next_value::<de::IgnoredAny>()?;}
            }
        } Ok(())
    }
}
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub(crate) struct Node { pub session_id:String, #[serde(skip_serializing)] pub cwd:String, pub parent_session_id:String, pub title:String, pub depth:usize, pub state:String, pub updated_at:i64 }
#[derive(Default, Clone, Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub(crate) struct TaskTree { pub nodes:Vec<Node>, pub running:usize, pub attention:usize, pub unknown:usize, pub limited:bool }
impl TaskTree {
    pub fn phase<'a>(&self,parent:&'a str)->&'a str {
        if matches!(parent,"ACTION_REQUIRED"|"FAILED"){parent} else if self.attention>0 {"ACTION_REQUIRED"} else if self.running>0||parent=="RUNNING" {"RUNNING"} else if self.unknown>0||(self.limited&&!self.nodes.is_empty()) {"UNCONFIRMED"} else {parent}
    }
}
fn metadata(path:&Path)->Option<Meta>{
    let mut m=Meta::default();
    let mut d=serde_json::Deserializer::from_reader(BufReader::new(File::open(path).ok()?.take(2*1024*1024)));
    if let Err(error)=d.deserialize_map(&mut m){let code=error.to_string();m.incomplete=!code.contains("RELATION_COMPLETE")&&!code.contains("RELATION_ENDED");}
    if !m.local.starts_with("local_")||uuid::Uuid::parse_str(&m.native).is_err()||m.cwd.is_empty(){return None;}
    m.title=m.title.chars().filter(|c|!c.is_control()&&!matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')).take(512).collect();
    Some(m)
}
fn collect(roots:&[PathBuf])->Vec<(PathBuf,Meta)>{
    crate::claude_code_desktop::session_files(roots).into_iter().filter_map(|(p,archived)|{
        let mut m=metadata(&p)?;m.archived|=archived.contains(&m.local);Some((p.parent()?.to_path_buf(),m))
    }).collect()
}
fn build(rows:&[(PathBuf,Meta)],sid:&str,cwd:&str,mut status:impl FnMut(&Meta)->(String,i64))->TaskTree {
    let mut tree=TaskTree::default();let mut unique=HashMap::<String,(String,String,Option<String>)>::new();
    let mut conflicts=HashSet::new();
    for (_,m) in rows {let sig=(m.native.clone(),m.cwd.clone(),m.parent.as_ref().map(|p|p.session_id.clone()));if let Some(old)=unique.insert(m.local.clone(),sig.clone()){if old!=sig{conflicts.insert(m.local.clone());}}}
    let Some((_,root))=rows.iter().find(|(_,m)|m.native==sid&&router_core::claude_code::same_root(&m.cwd,cwd)&&!conflicts.contains(&m.local))else{tree.limited=true;return tree};
    tree.limited=root.incomplete;
    let mut queue=vec![(root.local.clone(),sid.to_owned(),0usize)];let mut seen=HashSet::from([root.local.clone()]);let mut native_seen=HashSet::from([sid.to_owned()]);
    while let Some((local,parent,depth))=queue.pop(){
        for (_,m) in rows.iter().filter(|(_,m)|m.parent.as_ref().is_some_and(|p|p.session_id==local)) {
            if conflicts.contains(&m.local){tree.limited=true;tree.unknown+=1;continue;}
            if seen.contains(&m.local){continue;}
            if depth>=8||tree.nodes.len()>=64{tree.limited=true;continue;}
            if !native_seen.insert(m.native.clone()){tree.limited=true;tree.unknown+=1;continue;}
            seen.insert(m.local.clone());tree.limited|=m.incomplete;let (state,at)=if m.archived{("ARCHIVED".into(),0)}else{status(m)};
            match state.as_str(){"RUNNING"=>tree.running+=1,"WAITING_DECISION"|"WAITING_REPLY"|"FAILED"|"INTERRUPTED"=>tree.attention+=1,"UNKNOWN"=>tree.unknown+=1,_=>{}}
            tree.nodes.push(Node{session_id:m.native.clone(),cwd:m.cwd.clone(),parent_session_id:parent.clone(),title:if m.title.is_empty(){"Claude Code".into()}else{m.title.clone()},depth:depth+1,state,updated_at:at});
            queue.push((m.local.clone(),m.native.clone(),depth+1));
        }
    }
    fn append(parent:&str,nodes:&[Node],out:&mut Vec<Node>){
        let mut children:Vec<_>=nodes.iter().filter(|n|n.parent_session_id==parent).collect();
        children.sort_by(|a,b|a.title.cmp(&b.title).then_with(||a.session_id.cmp(&b.session_id)));
        for node in children {out.push(node.clone());append(&node.session_id,nodes,out);}
    }
    let mut ordered=Vec::new();append(sid,&tree.nodes,&mut ordered);tree.nodes=ordered;tree
}
type StatusCache=HashMap<PathBuf,((u64,SystemTime),Option<crate::claude_transcript::Status>)>;
pub(crate) fn read(core:&crate::host_application::RouterCore,sid:&str,cwd:&str)->TaskTree {
    #[cfg(test)] let rows=collect(&crate::claude_code_desktop::TEST_ROOTS.lock().map(|r|r.clone()).unwrap_or_default());
    #[cfg(not(test))] let rows={
        static CACHE:OnceLock<Mutex<Option<(Instant,Vec<(PathBuf,Meta)>)>>>=OnceLock::new();
        let Ok(mut cache)=CACHE.get_or_init(||Mutex::new(None)).lock()else{return TaskTree{limited:true,..Default::default()}};
        if cache.as_ref().is_none_or(|(at,_)|at.elapsed()>Duration::from_secs(5)){*cache=Some((Instant::now(),collect(&crate::claude_code_desktop::data_roots())));}
        cache.as_ref().unwrap().1.clone()
    };
    static STATUSES:OnceLock<Mutex<StatusCache>>=OnceLock::new();
    build(&rows,sid,cwd,|m|{
        let read=||->Option<crate::claude_transcript::Status>{
            let p=crate::claude_transcript::find(&crate::claude_transcript::projects_root()?,&m.native)?;let meta=std::fs::metadata(&p).ok()?;
            let stamp=(meta.len(),meta.modified().ok()?);let mut cache=STATUSES.get_or_init(||Mutex::new(HashMap::new())).lock().ok()?;
            if cache.len()>256{cache.clear();}
            if let Some((old,s))=cache.get(&p){if old==&stamp{return s.clone();}}
            let s=crate::claude_transcript::status(&p,&m.native);cache.insert(p,(stamp,s.clone()));s
        };
        if let Some(s)=read().filter(|s|s.cwd.as_ref().is_some_and(|r|router_core::claude_code::same_root(r,&m.cwd))){
            let pending=core.store.claude_owner_request_pending(&m.native,&m.cwd).unwrap_or(false);
            let approval=core.store.claude_session(&m.native).ok().is_some_and(|p|p.state=="WAITING_DECISION"&&p.turn_id.as_deref()==Some(&s.turn)&&router_core::claude_code::same_root(&p.cwd,&m.cwd));
            return ((if approval||pending{"WAITING_DECISION"}else if s.state=="COMPLETE"&&crate::claude_transcript::awaiting_reply(&m.native,&s.turn){"WAITING_REPLY"}else if s.state=="COMPLETE"{"TURN_ENDED"}else if s.state=="EMPTY"{"IDLE"}else{s.state}).into(),s.completed_at)
        } ("UNKNOWN".into(),0)
    })
}

#[cfg(test)] mod tests {
    use super::*;
    fn meta(local:&str,native:&str,parent:Option<&str>)->(PathBuf,Meta){(PathBuf::from("partition"),Meta{local:local.into(),native:native.into(),cwd:std::env::temp_dir().to_string_lossy().into(),parent:parent.map(|p|Spawn{session_id:p.into()}),..Default::default()})}
    #[test] fn aggregates_grandchildren_and_never_calls_a_turn_task_complete(){
        let rows=vec![meta("root","r",None),meta("child","c",Some("root")),meta("grand","g",Some("child"))];
        let t=build(&rows,"r",&rows[0].1.cwd,|m|(if m.native=="g"{"RUNNING"}else{"TURN_ENDED"}.into(),0));
        assert_eq!(t.nodes.len(),2);assert_eq!(t.running,1);assert_eq!(t.phase("COMPLETE"),"RUNNING");assert_eq!(t.nodes[1].depth,2);
    }
    #[test] fn missing_and_conflicting_relations_fail_closed(){
        let mut rows=vec![meta("root","r",None),meta("child","c",Some("root"))];let mut conflict=meta("child","other",Some("root"));conflict.0="other-partition".into();rows.push(conflict);
        let t=build(&rows,"r",&rows[0].1.cwd,|_|panic!("ambiguous identity"));assert!(t.limited);assert!(t.nodes.is_empty());assert_eq!(t.phase("COMPLETE"),"UNCONFIRMED");
    }
    #[test] fn relation_reader_retains_only_whitelisted_fields(){
        let text=r#"{"sessionId":"local_a","cliSessionId":"00000000-0000-4000-8000-000000000001","cwd":"fixture","remoteMcpServersConfig":{"secret":"not retained"},"completedTurns":[],"spawnedFrom":{"sessionId":"local_p","taskId":"x"},"messages":["do not consume"]}"#;
        let mut m=Meta::default();let mut d=serde_json::Deserializer::from_str(text);let error=d.deserialize_map(&mut m).unwrap_err();assert_eq!(m.parent.unwrap().session_id,"local_p");assert!(error.to_string().contains("RELATION_COMPLETE"));
    }
    #[test] #[ignore="explicit owner-native read-only acceptance; no model, plugin or store calls"]
    fn real_owner_task_tree_read_only(){
        let sid=std::env::var("AGBRIO_TREE_SESSION").expect("explicit exact native session required");
        let rows=collect(&crate::claude_code_desktop::data_roots());
        let cwd=rows.iter().find(|(_,m)|m.native==sid).expect("exact native session metadata").1.cwd.clone();
        let projects=std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".claude/projects");
        let t=build(&rows,&sid,&cwd,|m|{
            let status=crate::claude_transcript::find(&projects,&m.native).and_then(|p|crate::claude_transcript::status(&p,&m.native));
            status.filter(|s|s.cwd.as_ref().is_some_and(|p|router_core::claude_code::same_root(p,&m.cwd))).map(|s|((if s.state=="COMPLETE"{"TURN_ENDED"}else{s.state}).into(),s.completed_at)).unwrap_or(("UNKNOWN".into(),0))
        });
        assert!(t.nodes.len()>=4,"expected selected owner's task family");assert!(t.nodes.iter().any(|n|n.depth>=2));
        let path=crate::claude_transcript::find(&projects,&sid).unwrap();let first=crate::claude_transcript::page(&path,&sid,None).unwrap();
        let native_status=crate::claude_transcript::status(&path,&sid).unwrap();
        if native_status.reply.is_some(){assert!(first.turns.iter().any(|turn|turn.id==native_status.turn&&turn.messages.iter().any(|m|m.role=="assistant")),"native state and history must share exact turn identity, including meta peer inputs");}
        let mut cursor=first.next;let mut pages=1;let mut messages=first.turns.iter().map(|t|t.messages.len()).sum::<usize>();let mut collaboration=first.turns.iter().flat_map(|t|&t.messages).filter(|m|m.kind=="COLLABORATION").count();
        let mut ids:HashSet<_>=first.turns.iter().flat_map(|t|t.messages.iter().map(|m|m.id.clone())).collect();
        while let Some(before)=cursor {assert!(pages<256);let page=crate::claude_transcript::page(&path,&sid,Some(before)).unwrap();assert!(page.next.is_none_or(|n|n<before));messages+=page.turns.iter().map(|t|t.messages.len()).sum::<usize>();collaboration+=page.turns.iter().flat_map(|t|&t.messages).filter(|m|m.kind=="COLLABORATION").count();ids.extend(page.turns.iter().flat_map(|t|t.messages.iter().map(|m|m.id.clone())));cursor=page.next;pages+=1;}
        assert_eq!(ids,crate::claude_transcript::reference_ids(&path,&sid),"paginated public/coordination identities must cover the full native record");
        assert!(collaboration>0);println!("read-only native acceptance: nodes={} depth={} running={} unknown={} limited={} pages={} messages={} coordination={}",t.nodes.len(),t.nodes.iter().map(|n|n.depth).max().unwrap_or(0),t.running,t.unknown,t.limited,pages,messages,collaboration);
    }
}
