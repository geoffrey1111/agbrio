//! Reuses the 2026-09-24 Desktop catalogue projection, not a transcript reader.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Membership {
    Project(String),
    Projectless,
    Unconfirmed,
}
#[derive(Deserialize)]
struct State {
    #[serde(rename = "local-projects")]
    projects: BTreeMap<String, Project>,
    #[serde(rename = "thread-project-assignments")]
    assignments: BTreeMap<String, Option<Assignment>>,
    #[serde(rename = "projectless-thread-ids")]
    projectless: Vec<String>,
    #[serde(rename = "sidebar-project-thread-orders")]
    orders: BTreeMap<String, Order>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Project {
    pub name: String,
    #[serde(rename = "rootPaths", default)]
    pub roots: Vec<String>,
}
#[derive(Deserialize)]
struct Assignment {
    #[serde(rename = "projectKind")]
    kind: String,
    #[serde(rename = "projectId")]
    id: String,
}
#[derive(Deserialize)]
struct Order {
    #[serde(rename = "threadIds", default)]
    threads: Vec<String>,
}
#[derive(Default)]
pub struct DesktopCatalog {
    pub projects: BTreeMap<String, Project>,
    pub memberships: BTreeMap<String, Membership>,
}
fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 2048 {
        Err("DESKTOP_CATALOG_SCHEMA_UNVERIFIED".into())
    } else {
        Ok(())
    }
}
fn record(m: &mut BTreeMap<String, Membership>, id: String, next: Membership) {
    let merged = match m.get(&id) {
        None => next,
        Some(old) if *old == next => next,
        _ => Membership::Unconfirmed,
    };
    m.insert(id, merged);
}
pub fn read(path: &Path) -> Result<DesktopCatalog, String> {
    let f = File::open(path).map_err(|_| "DESKTOP_CATALOG_UNAVAILABLE")?;
    if f.metadata()
        .map_err(|_| "DESKTOP_CATALOG_UNAVAILABLE")?
        .len()
        > 16 * 1024 * 1024
    {
        return Err("DESKTOP_CATALOG_SCHEMA_UNVERIFIED".into());
    }
    // Typed streaming Serde skips all unrelated fields without materializing them.
    let s: State = serde_json::from_reader(BufReader::new(f.take(16 * 1024 * 1024 + 1)))
        .map_err(|_| "DESKTOP_CATALOG_SCHEMA_UNVERIFIED")?;
    project(s)
}
fn project(s: State) -> Result<DesktopCatalog, String> {
    let mut m = BTreeMap::new();
    for (id, p) in &s.projects {
        valid_id(id)?;
        if p.name.len() > 2048
            || p.roots.len() > 128
            || p.roots.iter().any(|r| r.is_empty() || r.len() > 32768)
        {
            return Err("DESKTOP_CATALOG_SCHEMA_UNVERIFIED".into());
        }
    }
    for (id, a) in s.assignments {
        valid_id(&id)?;
        let next = match a {
            Some(a) if a.kind == "local" && valid_id(&a.id).is_ok() => Membership::Project(a.id),
            _ => Membership::Unconfirmed,
        };
        record(&mut m, id, next);
    }
    for (pid, o) in s.orders {
        valid_id(&pid)?;
        if o.threads.len() > 10000 {
            return Err("DESKTOP_CATALOG_SCHEMA_UNVERIFIED".into());
        }
        for id in o.threads {
            valid_id(&id)?;
            record(&mut m, id, Membership::Project(pid.clone()));
        }
    }
    if s.projectless.len() > 10000 {
        return Err("DESKTOP_CATALOG_SCHEMA_UNVERIFIED".into());
    }
    for id in s.projectless {
        valid_id(&id)?;
        record(&mut m, id, Membership::Projectless);
    }
    Ok(DesktopCatalog {
        projects: s.projects,
        memberships: m,
    })
}
pub fn current() -> Result<DesktopCatalog, String> {
    let home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| std::path::PathBuf::from(p).join(".codex"))
        })
        .ok_or("DESKTOP_CATALOG_UNAVAILABLE")?;
    read(&home.join(".codex-global-state.json"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_membership_conflict_projectless_and_duplicate_project_labels() {
        let s = json!({"local-projects":{"a":{"name":"同名","rootPaths":["D:/a"]},"b":{"name":"同名","rootPaths":["D:/b"]}},"thread-project-assignments":{"good":{"projectKind":"local","projectId":"a"},"bad":{"projectKind":"local","projectId":"a"},"null":null},"projectless-thread-ids":["free"],"sidebar-project-thread-orders":{"a":{"threadIds":["good"]},"b":{"threadIds":["bad"]}},"unrelated":{"auth":"ignored","conversation":"ignored"}});
        let p = project(serde_json::from_value(s).unwrap()).unwrap();
        assert_eq!(p.projects.len(), 2);
        assert_eq!(p.memberships["good"], Membership::Project("a".into()));
        assert_eq!(p.memberships["bad"], Membership::Unconfirmed);
        assert_eq!(p.memberships["free"], Membership::Projectless);
        assert_eq!(p.memberships["null"], Membership::Unconfirmed);
        assert!(!p.memberships.contains_key("missing"));
    }
    #[test]
    fn missing_or_drifted_projection_does_not_invent_projectless() {
        assert!(serde_json::from_str::<State>("{}").is_err());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.json");
        std::fs::write(&p, "{\"local-projects\":[]}").unwrap();
        assert!(read(&p).is_err());
    }
}
