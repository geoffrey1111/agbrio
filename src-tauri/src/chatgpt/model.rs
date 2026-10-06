use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundConversation {
    pub id: String,
    pub title: Option<String>,
}

/// Ephemeral, read-only materialization of the currently visible branch of an
/// exact ChatGPT conversation. It is never written to Router SQLite storage.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHistory {
    pub id: String,
    pub title: String,
    pub branch_scope: String,
    pub completeness: String,
    pub messages: Vec<ConversationHistoryMessage>,
    pub diagnostics: ConversationHistoryDiagnostics,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHistoryMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub blocks: Vec<serde_json::Value>,
    #[serde(default)]
    pub code_blocks: Vec<serde_json::Value>,
    #[serde(default)]
    pub resources: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHistoryDiagnostics {
    pub beginning_reached: bool,
    pub end_reached: bool,
    pub beginning_steps: u32,
    pub end_steps: u32,
    pub initial_dom_message_count: u32,
    pub scroll_target: String,
    pub viewport_height: u32,
    pub scrollable_height: u32,
    pub end_last_scroll_top: u32,
    pub end_last_scrollable_height: u32,
    pub end_stalled_steps: u32,
    pub scroll_restored: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffStatus {
    Ready,
    Approved,
    Sending,
    Sent,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelayCandidateKind {
    ExplicitMarker,
    ContextualCodeBlock,
    GenericCodeBlock,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayCandidate {
    pub id: String,
    pub kind: RelayCandidateKind,
    pub confidence: String,
    pub text: String,
}

/// Selectable source slices, never execution or routing authority. Unlike the
/// legacy candidate projection, content and line endings stay intact. Proven
/// instruction fence/outer quote wrappers are presentation, not payload text.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayTextBlock {
    pub id: String,
    pub kind: String,
    pub recommended: bool,
    pub text: String,
}

pub fn relay_text_blocks(source: &str) -> Vec<RelayTextBlock> {
    let mut offsets = Vec::new();
    let mut pos = 0;
    let lines: Vec<&str> = source.split_inclusive('\n').map(|line| {
        offsets.push(pos); pos += line.len(); line.trim_end_matches(['\r', '\n'])
    }).collect();
    offsets.push(source.len());
    struct Span { start: usize, end: usize, kind: &'static str, recommended: bool }
    let mut result: Vec<Span> = Vec::new();
    let mut index = 0;
    let push = |out: &mut Vec<Span>, start: usize, end: usize, kind: &'static str, recommended: bool| {
        let text = source[offsets[start]..offsets[end]].trim_end_matches(['\r', '\n']);
        if !text.trim().is_empty() {
            out.push(Span { start: offsets[start], end: offsets[start] + text.len(), kind, recommended });
        }
    };
    while index < lines.len() {
        // Bounded card count without dropping the remainder of the original.
        if result.len() >= 127 { push(&mut result, index, lines.len(), "PROSE", false); break; }
        if lines[index].trim().is_empty() { index += 1; continue; }
        let current = lines[index].trim();
        if current.starts_with('>') {
            let start=index;
            while index<lines.len()&&lines[index].trim_start().starts_with('>'){index+=1;}
            let intro=lines[..start].iter().rev().find(|s|!s.trim().is_empty()).copied().unwrap_or("").trim().trim_end_matches('*').trim_end();
            let recommended=contains_codex_relay_marker(intro)&&intro.ends_with([':', '：']);
            push(&mut result,start,index,if recommended{"QUOTED_INSTRUCTION"}else{"QUOTE"},recommended);
            continue;
        }
        let close_marker = match current { "<CODEX_HANDOFF>" => Some("</CODEX_HANDOFF>"), "<CHATGPT_HANDOFF>" => Some("</CHATGPT_HANDOFF>"), _ => None };
        let fence = fence_signature(current);
        let closing = (close_marker.is_some() || fence.is_some()).then(|| (index + 1..lines.len()).find(|&at| {
            if let Some(close) = close_marker { lines[at].trim() == close }
            else if let Some((character, count)) = fence {
                let line = lines[at].trim();
                line.chars().all(|c| c == character) && line.len() >= count
            } else { false }
        })).flatten();
        if let Some(end) = closing {
            // Nested marker pairs are ambiguous and remain manual text.
            let nested = close_marker.is_some() && lines[index + 1..end].iter().any(|s| s.trim() == current);
            let recommended = !nested && (close_marker.is_some() || contains_codex_relay_marker(&adjacent_prose(&lines, index)));
            push(&mut result, index + 1, end, if recommended { "INSTRUCTION" } else { "CODE" }, recommended);
            index = end + 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < lines.len() && !lines[index].trim().is_empty() && !lines[index].trim_start().starts_with('>') && fence_signature(lines[index].trim()).is_none() && !matches!(lines[index].trim(), "<CODEX_HANDOFF>" | "<CHATGPT_HANDOFF>") { index += 1; }
        push(&mut result, start, index, "PROSE", false);
    }
    // Instructions/code are indivisible. Coalesce surrounding prose into a few
    // contiguous source slices; never reorder, summarize, or discard originals.
    let has_special = result.iter().any(|s| s.kind != "PROSE");
    let plain_target = if has_special { 1 } else { result.len().min(4).max(1) };
    let per_group = result.len().div_ceil(plain_target);
    let mut grouped: Vec<Span> = Vec::new();
    let mut in_group = 0;
    for span in result {
        let merge = span.kind == "PROSE" && grouped.last().is_some_and(|last| last.kind == "PROSE")
            && (has_special || in_group < per_group);
        if merge {
            grouped.last_mut().unwrap().end = span.end;
            in_group += 1;
        } else {
            grouped.push(span);
            in_group = 1;
        }
    }
    grouped.into_iter().enumerate().map(|(i, s)| RelayTextBlock {
        id: format!("source-block-{}", i + 1), kind: match s.kind{"QUOTED_INSTRUCTION"=>"INSTRUCTION","QUOTE"=>"PROSE",other=>other}.into(), recommended: s.recommended,
        text: if s.kind=="QUOTED_INSTRUCTION"{source[s.start..s.end].split_inclusive('\n').map(|line|{let line=line.trim_start_matches([' ','\t']);let line=line.strip_prefix('>').unwrap_or(line);line.strip_prefix(' ').unwrap_or(line)}).collect()}else{source[s.start..s.end].into()},
    }).collect()
}

fn fence_signature(line: &str) -> Option<(char, usize)> {
    let character = line.chars().next()?;
    if character != '`' && character != '~' { return None; }
    let count = line.chars().take_while(|&c| c == character).count();
    (count >= 3).then_some((character, count))
}

/// Deterministic, text-only Markdown segmentation. It intentionally does not
/// inspect browser DOM or use an LLM to infer a relay instruction.
pub fn relay_candidates(markdown: &str) -> Vec<RelayCandidate> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut candidates = Vec::new();
    let mut index = 0usize;
    let mut candidate_number = 0usize;

    while index < lines.len() {
        if lines[index].trim() == "<CODEX_HANDOFF>" {
            let start = index + 1;
            index += 1;
            while index < lines.len() && lines[index].trim() != "</CODEX_HANDOFF>" {
                index += 1;
            }
            if index < lines.len() {
                let text = lines[start..index].join("\n").trim().to_string();
                if !text.is_empty() {
                    candidate_number += 1;
                    candidates.push(RelayCandidate {
                        id: format!("relay-candidate-{candidate_number}"),
                        kind: RelayCandidateKind::ExplicitMarker,
                        confidence: "HIGH".to_string(),
                        text,
                    });
                }
                index += 1;
                continue;
            }
            // An unclosed marker remains ordinary prose and cannot create a candidate.
            index = start;
        }

        if is_fence(lines[index]) {
            let context = adjacent_prose(&lines, index);
            let start = index + 1;
            index += 1;
            while index < lines.len() && !is_fence(lines[index]) {
                index += 1;
            }
            if index < lines.len() {
                let text = lines[start..index].join("\n").trim().to_string();
                if !text.is_empty() {
                    candidate_number += 1;
                    let contextual = contains_codex_relay_marker(&context);
                    candidates.push(RelayCandidate {
                        id: format!("relay-candidate-{candidate_number}"),
                        kind: if contextual {
                            RelayCandidateKind::ContextualCodeBlock
                        } else {
                            RelayCandidateKind::GenericCodeBlock
                        },
                        confidence: if contextual {
                            "HIGH".to_string()
                        } else {
                            "MANUAL".to_string()
                        },
                        text,
                    });
                }
                index += 1;
                continue;
            }
        }
        index += 1;
    }
    candidates
}

/// Returns a handoff payload only when the whole document proves exactly one
/// well-formed, non-empty marker block. This is intentionally stricter than
/// the legacy candidate extractor: contextual prose and fenced code blocks
/// never become mobile routing authority.
pub fn exact_single_handoff_block(markdown: &str, marker: &str) -> Option<String> {
    let open = format!("<{marker}>");
    let close = format!("</{marker}>");
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut start: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line == open {
            if start.replace(index + 1).is_some() {
                return None;
            }
        } else if line == close {
            let Some(from) = start.take() else {
                return None;
            };
            let text = lines[from..index].join("\n").trim().to_string();
            if text.is_empty() {
                return None;
            }
            blocks.push(text);
        }
    }
    if start.is_some() || blocks.len() != 1 {
        return None;
    }
    Some(blocks.remove(0))
}

fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

fn adjacent_prose(lines: &[&str], fence_index: usize) -> String {
    // Ordinary Markdown commonly leaves one or more blank lines between a
    // natural-language relay lead-in and its fenced instruction.  Skip only
    // that immediately adjacent whitespace, then inspect exactly the prior
    // prose paragraph.  We never scan across a prose paragraph or another
    // code fence, so distant narrative cannot promote a generic block.
    let mut end = fence_index;
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && !lines[start - 1].trim().is_empty() && !is_fence(lines[start - 1]) {
        start -= 1;
    }
    lines[start..end].join("\n")
}

fn contains_codex_relay_marker(prose: &str) -> bool {
    let normalized = prose
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let markers = [
        "发给codex",
        "发送给codex",
        "给codex",
        "交给codex",
        "复制给codex",
        "把下面这条发给codex",
        "把下面这段发给codex",
        "发给执行端",
        "转给执行端",
        "交给执行端",
        "发给控制端",
        "转给控制端",
        "发给管理端",
        "发给chatgpt",
        "发给原对话",
        "发给原执行对话",
        "发送给原执行对话",
        "转给原执行对话",
        "发给原控制对话",
        "发给原管理对话",
        "发送给原对话",
        "转给原对话",
    ];

    markers.iter().any(|marker| {
        normalized
            .match_indices(marker)
            .any(|(marker_start, _)| !has_negated_relay_context(&normalized, marker_start))
    })
}

fn has_negated_relay_context(normalized_prose: &str, marker_start: usize) -> bool {
    // A false negative remains a manual candidate; a false HIGH relay is the
    // risk to avoid. Restrict the check to nearby prose so unrelated earlier
    // wording does not suppress an otherwise explicit instruction.
    let nearby_prefix = normalized_prose[..marker_start]
        .chars()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    [
        "不要", "别", "禁止", "不应", "不能", "不必", "无需", "不是", "并非", "不可以", "不建议", "不适合", "不允许", "无法",
    ]
    .iter()
    .any(|negation| nearby_prefix.contains(negation))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffAttachment {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub actual_sha256: Option<String>,
    /// Review-time declared-versus-local integrity result. This is distinct
    /// from `actual_sha256`, which is retained for pre-send mutation checks.
    #[serde(default)]
    pub integrity_status: Option<String>,
}

/// Fresh local evidence from the final check immediately before provider file
/// import. It is separate from review-time integrity classification.
#[derive(Clone, Debug)]
pub struct PreDispatchAttachmentEvidence {
    pub attachment_id: String,
    pub review_sha256: Option<String>,
    pub integrity_status: Option<String>,
    pub send_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffDraft {
    pub workstream_id: String,
    pub source_codex_thread_id: String,
    pub destination_chatgpt_conversation_id: String,
    #[serde(default)]
    pub original_text: String,
    pub message: String,
    pub attachments: Vec<HandoffAttachment>,
    pub status: HandoffStatus,
}

/// A session-only reverse handoff. It deliberately reuses `HandoffStatus` and
/// the same READY -> APPROVED -> SENDING lifecycle as the outbound draft.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseHandoffDraft {
    pub workstream_id: String,
    pub source_chatgpt_conversation_id: String,
    pub source_response_identity: String,
    pub source_candidate_id: Option<String>,
    pub destination_codex_thread_id: String,
    pub message: String,
    #[serde(default)]
    pub attachments: Vec<HandoffAttachment>,
    pub status: HandoffStatus,
}

impl HandoffDraft {
    pub fn approve(&mut self) -> Result<(), String> {
        if self.status != HandoffStatus::Ready {
            return Err("Only a READY handoff can be approved".to_string());
        }
        if self.workstream_id.trim().is_empty()
            || self.source_codex_thread_id.trim().is_empty()
            || self.destination_chatgpt_conversation_id.trim().is_empty()
            || self.message.trim().is_empty()
        {
            return Err(
                "A handoff requires a Workstream, source thread, destination conversation, and message"
                    .to_string(),
            );
        }
        self.status = HandoffStatus::Approved;
        Ok(())
    }

    pub fn sending(&mut self) -> Result<(), String> {
        if self.status != HandoffStatus::Approved {
            return Err("Only an APPROVED handoff can be sent".to_string());
        }
        self.status = HandoffStatus::Sending;
        Ok(())
    }
}

impl ReverseHandoffDraft {
    pub fn approve(&mut self) -> Result<(), String> {
        if self.status != HandoffStatus::Ready {
            return Err("Only a READY handoff can be approved".to_string());
        }
        if self.workstream_id.trim().is_empty()
            || self.source_chatgpt_conversation_id.trim().is_empty()
            || self.source_response_identity.trim().is_empty()
            || self.destination_codex_thread_id.trim().is_empty()
            || self.message.trim().is_empty()
        {
            return Err("A reverse handoff requires a Workstream, completed ChatGPT source identity, destination thread, and message".to_string());
        }
        self.status = HandoffStatus::Approved;
        Ok(())
    }

    pub fn sending(&mut self) -> Result<(), String> {
        if self.status != HandoffStatus::Approved {
            return Err("Only an APPROVED handoff can be sent".to_string());
        }
        self.status = HandoffStatus::Sending;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_requires_explicit_approval_before_send() {
        let mut draft = HandoffDraft {
            workstream_id: "workstream".into(),
            source_codex_thread_id: "thread".into(),
            destination_chatgpt_conversation_id: "conversation".into(),
            original_text: "review".into(),
            message: "review".into(),
            attachments: vec![],
            status: HandoffStatus::Ready,
        };
        assert!(draft.sending().is_err());
        draft.approve().unwrap();
        draft.sending().unwrap();
        assert_eq!(draft.status, HandoffStatus::Sending);
    }

    #[test]
    fn reverse_handoff_requires_completed_source_and_explicit_approval() {
        let mut draft = ReverseHandoffDraft {
            workstream_id: "workstream".into(),
            source_chatgpt_conversation_id: "conversation".into(),
            source_response_identity: "request".into(),
            source_candidate_id: Some("relay-candidate-1".into()),
            destination_codex_thread_id: "thread".into(),
            message: "reviewed reply".into(),
            attachments: vec![],
            status: HandoffStatus::Ready,
        };
        assert!(draft.sending().is_err());
        draft.approve().unwrap();
        draft.sending().unwrap();
        assert_eq!(draft.status, HandoffStatus::Sending);
    }

    #[test]
    fn reverse_handoff_rejects_partial_response_without_identity() {
        let mut draft = ReverseHandoffDraft {
            workstream_id: "workstream".into(),
            source_chatgpt_conversation_id: "conversation".into(),
            source_response_identity: "".into(),
            source_candidate_id: None,
            destination_codex_thread_id: "thread".into(),
            message: "partial".into(),
            attachments: vec![],
            status: HandoffStatus::Ready,
        };
        assert!(draft.approve().is_err());
    }

    #[test]
    fn contextual_marker_extracts_only_the_adjacent_code_block() {
        let candidates = relay_candidates("Explanation for the user.\n\n请把下面这条发给 Codex：\n```\nAuthorize SB-002\n```\n\nMore explanation.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::ContextualCodeBlock);
        assert_eq!(candidates[0].text, "Authorize SB-002");
    }

    #[test]
    fn exact_mobile_marker_requires_one_complete_nonempty_boundary() {
        assert_eq!(
            exact_single_handoff_block(
                "Prose\n<CODEX_HANDOFF>\nOnly this\n</CODEX_HANDOFF>\nMore prose",
                "CODEX_HANDOFF",
            ),
            Some("Only this".into())
        );
        for invalid in [
            "```\ncode\n```",
            "<CODEX_HANDOFF>\n</CODEX_HANDOFF>",
            "<CODEX_HANDOFF>\none",
            "</CODEX_HANDOFF>",
            "<CODEX_HANDOFF>\none\n</CODEX_HANDOFF>\n<CODEX_HANDOFF>\ntwo\n</CODEX_HANDOFF>",
        ] {
            assert_eq!(
                exact_single_handoff_block(invalid, "CODEX_HANDOFF"),
                None,
                "{invalid}"
            );
        }
    }

    #[test]
    fn common_chatgpt_brief_keeps_analysis_and_progress_outside_the_codex_instruction() {
        let candidates = relay_candidates(
            "## 分析结论\n\n先完成真实循环，再处理视觉细节。\n\n任务进度：V0-014 正在进行。\n\n下面直接交给 Codex。\n\n```text\nCONTINUE V0-014\n\n只继续当前分支；不要提交。\n```",
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::ContextualCodeBlock);
        assert_eq!(candidates[0].confidence, "HIGH");
        assert_eq!(
            candidates[0].text,
            "CONTINUE V0-014\n\n只继续当前分支；不要提交。"
        );
    }

    #[test]
    fn generic_code_block_is_manual_not_high_confidence() {
        let candidates = relay_candidates("Here is a code example:\n```\nconst value = 1;\n```");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::GenericCodeBlock);
        assert_eq!(candidates[0].confidence, "MANUAL");
    }

    #[test]
    fn negated_relay_instruction_keeps_code_block_manual() {
        let candidates = relay_candidates("不要把下面这段发给 Codex：\n```text\nexample\n```");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::GenericCodeBlock);
        assert_eq!(candidates[0].confidence, "MANUAL");
        assert_eq!(candidates[0].text, "example");
    }

    #[test]
    fn not_for_codex_context_keeps_code_block_manual() {
        let candidates = relay_candidates("这段不是给 Codex 的，只是示例：\n```text\nexample\n```");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::GenericCodeBlock);
        assert_eq!(candidates[0].confidence, "MANUAL");
    }

    #[test]
    fn post_block_above_reference_remains_manual() {
        let candidates = relay_candidates(
            "```text\nSTATUS = OWNER_PHONE_UAT_READY\n```\n\n把上面这条发给 Codex，然后让它自己连续跑。",
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::GenericCodeBlock);
        assert_eq!(candidates[0].confidence, "MANUAL");
        assert_eq!(candidates[0].text, "STATUS = OWNER_PHONE_UAT_READY");
    }

    #[test]
    fn multiple_contextual_blocks_remain_separate_candidates() {
        let candidates =
            relay_candidates("发给 Codex：\n```\nfirst\n```\n\n交给 Codex：\n```\nsecond\n```");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].text, "first");
        assert_eq!(candidates[1].text, "second");
    }

    #[test]
    fn explicit_marker_extracts_exact_interior_without_markers() {
        let candidates =
            relay_candidates("For the user.\n<CODEX_HANDOFF>\nDo the task\n</CODEX_HANDOFF>");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, RelayCandidateKind::ExplicitMarker);
        assert_eq!(candidates[0].text, "Do the task");
    }
}

#[cfg(test)]
mod relay_block_tests {
    use super::*;
    #[test]
    fn original_execution_quote_is_separate_from_intro_and_preserves_instruction_bytes(){
        let instruction="按研究提交 `dcd091d1300257de65f33a488445e3871223e913` 的 `HANDOFF_PROOF_ERRATUM_ACCEPTED_20261005.md` 接续，落实版本化修订后执行11项校准。";
        let source=format!("独立审查完成。\r\n\r\n可直接发给原执行对话：\r\n\r\n> {instruction}\r\n>\r\n> 完成后回报。\r\n\r\n后续说明。");
        let blocks=relay_text_blocks(&source);
        assert_eq!(blocks.len(),3);
        assert!(blocks[0].text.contains("可直接发给原执行对话："));
        assert_eq!(blocks[1].kind,"INSTRUCTION");assert!(blocks[1].recommended);
        assert_eq!(blocks[1].text,format!("{instruction}\r\n\r\n完成后回报。"));
        assert!(!blocks[1].text.contains("可直接发给"));
        assert_eq!(blocks[2].text,"后续说明。");
        for intro in ["不要发给原执行对话：","下面是引用示例："]{
            let blocks=relay_text_blocks(&format!("{intro}\n> example"));
            assert!(!blocks.last().unwrap().recommended);
            assert_eq!(blocks.last().unwrap().text,"> example");
        }
        let inline=relay_text_blocks("可直接发给原执行对话：\n> 第一条。\n> > 内层引用。\n\n报告。");
        assert_eq!(inline[1].text,"第一条。\n> 内层引用。");
    }
    #[test]
    fn source_slices_keep_chinese_crlf_and_indentation_and_select_only_marked_instruction() {
        let source = "独立复审通过。\r\n\r\n可直接发给原对话：\r\n\r\n```text\r\n  请读取原合同。\r\n\r\n完成后回报。\r\n```\r\n\r\n最后的汇报。";
        let blocks = relay_text_blocks(source);
        assert_eq!(blocks.len(), 3);
        let instruction = &blocks[1];
        assert!(instruction.recommended);
        assert_eq!(instruction.kind, "INSTRUCTION");
        assert_eq!(instruction.text, "  请读取原合同。\r\n\r\n完成后回报。");
        assert!(blocks.iter().all(|b| source.contains(&b.text)));
        assert_eq!(relay_candidates(source)[0].confidence, "HIGH");
    }
    #[test]
    fn multiple_middle_blocks_are_distinct_and_examples_and_negations_are_manual() {
        let blocks = relay_text_blocks("结论\n\n发给执行端：\n\n```text\nfirst\n```\n\n不要把下面的内容发给原对话：\n\n~~~text\nexample\n~~~\n\n代码示例：\n\n````text\n```nested\nbody\n```\n````\n\n<CОDEX_HANDOFF>\nnot an ASCII marker\n</CОDEX_HANDOFF>");
        let candidates: Vec<_> = blocks.iter().filter(|b| b.kind != "PROSE").collect();
        assert_eq!(candidates.len(), 3);
        assert!(candidates[0].recommended);
        assert!(!candidates[1].recommended);
        assert!(!candidates[2].recommended);
        assert_eq!(candidates[2].text, "```nested\nbody\n```");
    }
    #[test]
    fn unclosed_blocks_are_never_recommended_and_large_original_is_not_dropped() {
        assert!(relay_text_blocks("发给执行端：\n\n```text\nunfinished").iter().all(|b| !b.recommended));
        let source = (0..200).map(|n| format!("paragraph {n}\n\n")).collect::<String>();
        let blocks = relay_text_blocks(&source);
        assert!((3..=5).contains(&blocks.len()));
        assert!(blocks.last().unwrap().text.contains("paragraph 199"));
    }
}

#[cfg(test)]
mod coarse_relay_tests {
 use super::*;
 #[test]
 fn report_is_coalesced_around_an_indivisible_instruction_in_original_order() {
  let report=(0..12).map(|i|format!("Report {i}.\r\n\r\n")).collect::<String>();
  let source=format!("{report}可直接发给原对话：\r\n\r\n```text\r\n  text\r\n\r\n第二条指令。\r\n```\r\n\r\n后续说明。\r\n\r\n仍需审查。");
  let blocks=relay_text_blocks(&source);assert_eq!(blocks.len(),3);assert_eq!(blocks[0].text,format!("{report}可直接发给原对话："));assert!(blocks[1].recommended);assert_eq!(blocks[1].text,"  text\r\n\r\n第二条指令。");assert_eq!(blocks[2].text,"后续说明。\r\n\r\n仍需审查。");
 }
 #[test]
 fn generic_long_prose_has_few_complete_source_slices_not_one_card_per_paragraph() {
  let source=(0..24).map(|i|format!("## Topic {i}\nBody {i}.\n\n")).collect::<String>();let blocks=relay_text_blocks(&source);assert_eq!(blocks.len(),4);assert!(blocks.iter().all(|b|source.contains(&b.text)));assert_eq!(blocks.iter().map(|b|b.text.clone()).collect::<Vec<_>>().join("\n\n"),source.trim_end());
 }
}
