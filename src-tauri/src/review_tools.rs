//! Review-domain handlers. Input contracts and atomic candidate publication live in harness::tools.
use crate::{
    db::Database,
    harness::tools::{ErrorCode, ToolError},
    review_agent::{ReviewLibrary, ReviewQuestion, ReviewRun, ToolCall},
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    mode: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceArgs {
    source_id: String,
    #[serde(default)]
    start_char: usize,
    #[serde(default)]
    max_chars: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuizArgs {
    topic: String,
    question: String,
    options: Vec<String>,
    correct_index: usize,
    explanation: String,
    #[serde(default)]
    source_ids: Vec<String>,
    #[serde(default)]
    card_id: Option<String>,
    #[serde(default)]
    memory_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GradeArgs {
    question_id: String,
}

fn args<T: serde::de::DeserializeOwned>(call: &ToolCall) -> Result<T, ToolError> {
    serde_json::from_str(&call.arguments).map_err(|_| {
        ToolError::new(
            ErrorCode::InvalidArguments,
            "工具参数格式错误，请按工具定义修正",
        )
    })
}
fn nonempty(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max
}

pub(crate) async fn execute(
    db: &Database,
    library: &dyn ReviewLibrary,
    run: &mut ReviewRun,
    call: &ToolCall,
    control: &crate::harness::execution::ExecutionControl,
) -> Result<(Value, Option<ReviewQuestion>), ToolError> {
    let result = match call.name.as_str() {
        "get_learning_progress" => {
            let input: Value = args(call)?;
            if input != json!({}) {
                return Err(ToolError::new(
                    ErrorCode::InvalidArguments,
                    "该工具不接受参数",
                ));
            }
            let progress = db.review_progress(run).map_err(ToolError::storage)?;
            for item in progress["review_memory"]["items"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(id) = item["id"].as_str() {
                    if !run.memory_ids.iter().any(|known| known == id) {
                        run.memory_ids.push(id.into());
                    }
                }
            }
            progress
        }
        "search_textbook" => {
            let input: SearchArgs = args(call)?;
            if !nonempty(&input.query, 1000)
                || !["keyword", "hybrid"].contains(&input.mode.as_str())
            {
                return Err(ToolError::new(
                    ErrorCode::InvalidArguments,
                    "检索词或检索方式无效",
                ));
            }
            let packet=library.call_controlled(json!({"op":"evidence","kb":run.scope.kb,"version":run.scope.version,"chapter":run.scope.chapter,"query":input.query,"mode":input.mode}), control.clone()).await?;
            if packet["kb"] != run.scope.kb || packet["version"] != run.scope.version {
                return Err(ToolError::new(
                    ErrorCode::ScopeMismatch,
                    "检索结果的教材版本不匹配",
                ));
            }
            let evidence = packet["evidence"]
                .as_array()
                .ok_or_else(|| ToolError::new(ErrorCode::InvalidResult, "检索结果缺少原文列表"))?;
            let mut matches = vec![];
            for item in evidence.iter().take(12) {
                let Some(text) = item["text"].as_str().filter(|t| nonempty(t, 10000)) else {
                    continue;
                };
                if !item["page"].as_u64().is_some_and(|p| p > 0)
                    || item["block_id"].as_str().is_none()
                {
                    continue;
                }
                let path: Vec<String> = serde_json::from_value(item["chapter_path"].clone())
                    .map_err(|_| ToolError::new(ErrorCode::InvalidResult, "原文章节格式无效"))?;
                if !path.starts_with(&run.scope.chapter_path) {
                    return Err(ToolError::new(
                        ErrorCode::ScopeMismatch,
                        "检索结果超出所选章节",
                    ));
                }
                let source = if let Some(old) = run
                    .sources
                    .iter()
                    .find(|s| s["block_id"] == item["block_id"] && s["page"] == item["page"])
                {
                    old.clone()
                } else {
                    if run.sources.len() >= 36 {
                        break;
                    }
                    let mut source = item.clone();
                    source["id"] = json!(format!("S{}", run.sources.len() + 1));
                    run.sources.push(source.clone());
                    source
                };
                matches.push(json!({"source_id":source["id"],"page":source["page"],"chapter_path":path,"excerpt":text.chars().take(400).collect::<String>()}));
            }
            json!({"matches":matches,"note":"可用 read_source 查看完整原文；没有匹配时可换词检索或明确使用模型知识。"})
        }
        "read_source" => {
            let input: SourceArgs = args(call)?;
            let mut source = run
                .sources
                .iter()
                .find(|s| s["id"] == input.source_id)
                .cloned()
                .ok_or_else(|| {
                    ToolError::new(ErrorCode::InvalidReference, "原文编号未由本次检索返回")
                })?;
            let chars = source["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .collect::<Vec<_>>();
            let length = input.max_chars.unwrap_or(2000);
            if !(1..=2000).contains(&length) || input.start_char >= chars.len() {
                return Err(ToolError::new(
                    ErrorCode::InvalidArguments,
                    "原文读取区间无效，max_chars 应为 1–2000，start_char 应在原文内",
                ));
            }
            let end = input.start_char.saturating_add(length).min(chars.len());
            source["text"] = json!(chars[input.start_char..end].iter().collect::<String>());
            source["start_char"] = json!(input.start_char);
            source["next_start_char"] = json!(if end < chars.len() { Some(end) } else { None });
            source["total_chars"] = json!(chars.len());
            source["has_more"] = json!(end < chars.len());
            source
        }
        "save_review_question" => {
            let input: QuizArgs = args(call)?;
            if run.questions.len() >= run.completion.contract.max_questions.min(3)
                || run.questions.iter().any(|q| q.correct.is_none())
            {
                return Err(ToolError::new(
                    ErrorCode::PreconditionFailed,
                    "请先等待并记录当前题目的真实回答；每次复习最多三题",
                ));
            }
            if !nonempty(&input.topic, 100)
                || !nonempty(&input.question, 1000)
                || !nonempty(&input.explanation, 4000)
                || !(2..=4).contains(&input.options.len())
                || input.correct_index >= input.options.len()
                || input.options.iter().any(|v| !nonempty(v, 600))
            {
                return Err(ToolError::new(
                    ErrorCode::InvalidArguments,
                    "题目、选项或正确选项编号无效",
                ));
            }
            let unique: std::collections::HashSet<_> =
                input.options.iter().map(|s| s.trim()).collect();
            if unique.len() != input.options.len() {
                return Err(ToolError::new(ErrorCode::InvalidArguments, "选项不能重复"));
            }
            if input
                .source_ids
                .iter()
                .any(|id| !run.sources.iter().any(|s| s["id"] == *id))
            {
                return Err(ToolError::new(
                    ErrorCode::InvalidReference,
                    "不能引用本次检索以外的原文编号；无依据时 source_ids 使用空数组",
                ));
            }
            if run.completion.contract.require_sources && input.source_ids.is_empty() {
                return Err(ToolError::new(
                    ErrorCode::SourceRequired,
                    "本次复习要求教材原文依据，请先检索原文；材料不足时不能保存无来源题目",
                ));
            }
            if let Some(id) = &input.card_id {
                let card = db
                    .learning_card(id)
                    .map_err(|_| ToolError::new(ErrorCode::InvalidReference, "关联卡片不存在"))?;
                if card["kb"] != run.scope.kb
                    || card["packet"]["version"] != run.scope.version
                    || !crate::learning::in_scope(&card, &run.scope.chapter_path)
                {
                    return Err(ToolError::new(
                        ErrorCode::ScopeMismatch,
                        "关联卡片超出当前复习范围",
                    ));
                }
            }
            if let Some(id) = &input.memory_id {
                if !run.memory_ids.contains(id)
                    || !db
                        .review_memory_get(id)
                        .map_err(ToolError::storage)?
                        .is_some_and(|m| m.in_scope(&run.scope))
                {
                    return Err(ToolError::new(
                        ErrorCode::InvalidReference,
                        "知识点编号必须来自当前范围的学习记录，请先读取进度",
                    ));
                }
            }
            if !run.due_memory_ids.is_empty()
                && !input.memory_id.as_ref().is_some_and(|id| {
                    run.due_memory_ids.contains(id)
                        && !run
                            .questions
                            .iter()
                            .any(|q| q.memory_id.as_ref() == Some(id))
                })
            {
                return Err(ToolError::new(
                    ErrorCode::PreconditionFailed,
                    "本轮需要复习指定的到期知识点，请带回对应 memory_id，每个知识点只出一道题",
                ));
            }
            let id = format!("q{}", run.questions.len() + 1);
            run.questions.push(ReviewQuestion {
                id: id.clone(),
                topic: input.topic,
                question: input.question,
                options: input.options,
                correct_index: input.correct_index,
                explanation: input.explanation,
                source_ids: input.source_ids,
                card_id: input.card_id,
                memory_id: input.memory_id,
                selected_index: None,
                correct: None,
            });
            json!({"question_id":id,"saved":true,"next":"等待用户作答，不要泄露答案"})
        }
        "record_quiz_result" => {
            let input: GradeArgs = args(call)?;
            let question = run
                .questions
                .iter_mut()
                .find(|q| q.id == input.question_id)
                .ok_or_else(|| ToolError::new(ErrorCode::InvalidReference, "题目不属于本次复习"))?;
            let selected = question.selected_index.ok_or_else(|| {
                ToolError::new(
                    ErrorCode::AnswerNotSubmitted,
                    "用户尚未提交答案，不能记录结果",
                )
            })?;
            let previously_recorded = question.correct.is_some();
            question.correct = Some(selected == question.correct_index);
            return Ok((
                json!({"question_id":question.id,"selected_index":selected,"correct":question.correct,"correct_index":question.correct_index,"explanation":question.explanation}),
                if previously_recorded {
                    None
                } else {
                    Some(question.clone())
                },
            ));
        }
        _ => {
            return Err(ToolError::new(
                ErrorCode::UnknownTool,
                "未知工具，请使用提供的工具列表",
            ))
        }
    };
    Ok((result, None))
}
