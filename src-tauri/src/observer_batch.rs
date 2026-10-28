//! Fair, bounded passive reads. Never starts a task or creates a writer.
use std::collections::HashSet;
pub(crate) const WATCH_READS_PER_PASS:usize=20;
pub(crate) fn next_ids<'a>(ids:impl Iterator<Item=&'a str>,after:&mut Option<String>,limit:usize)->HashSet<String>{
    let mut ids:Vec<String>=ids.map(str::to_owned).collect();ids.sort();ids.dedup();
    if ids.is_empty()||limit==0{return HashSet::new()}
    let start=after.as_ref().map(|last|ids.partition_point(|id|id<=last)).unwrap_or(0)%ids.len();
    ids.rotate_left(start);ids.truncate(limit);
    *after=ids.last().cloned();ids.into_iter().collect()
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn bounded_batches_revisit_every_identity_despite_input_order_and_removal(){
  let mut ids:Vec<String>=(0..55).map(|i|format!("watch-{i:03}")).collect();let mut cursor=None;let mut seen=HashSet::new();
  for _ in 0..3{ids.reverse();let batch=next_ids(ids.iter().map(String::as_str),&mut cursor,20);assert_eq!(batch.len(),20);seen.extend(batch);}
  assert_eq!(seen.len(),55);ids.retain(|id|!id.ends_with("004"));assert_eq!(next_ids(ids.iter().map(String::as_str),&mut cursor,20).len(),20);
 }
 #[test]fn empty_and_duplicate_inputs_do_not_expand_the_budget(){let mut cursor=None;assert!(next_ids([].into_iter(),&mut cursor,20).is_empty());assert_eq!(next_ids(["a","a","b"].into_iter(),&mut cursor,20).len(),2);assert!(next_ids(["a"].into_iter(),&mut cursor,0).is_empty());}
}
