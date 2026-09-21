//! Short, resumable learning sessions for ordinary knowledge cards (no PDF runtime).
use crate::{harness::policy::RunControl, providers::ProviderContext};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROMPT_VERSION: &str = "study-companion-v1";
pub const MAX_STEPS: usize = 6;
pub const MAX_QUESTIONS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyAnswer {
    pub kind: String,
    pub text: String,
    pub card_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<crate::search::types::SearchAnswer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyQuestion {
    pub id: String,
    pub step_id: String,
    pub question: String,
    pub created_at: String,
    pub answer: Option<StudyAnswer>,
    #[serde(default)]
    pub feedback: Option<String>,
    #[serde(default)]
    pub doubt_id: Option<String>,
    #[serde(default)]
    pub previous_answers: Vec<StudyAnswer>,
    #[serde(default)]
    pub reply_to_question_id: Option<String>,
    #[serde(default)]
    pub clarification_replies: Vec<StudyClarification>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<crate::study_search::StudySearchRun>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyClarification {
    pub prompt: String,
    pub reply: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyDoubtTarget {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub reason: String,
}

impl StudyQuestion {
    pub(crate) fn doubt_reason(&self) -> String {
        format!("你上次对“{}”反馈“还没懂”。", self.question)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyQuiz {
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    pub selected: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyStep {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub reason: String,
    pub card_id: Option<String>,
    pub quiz: Option<StudyQuiz>,
    pub feedback: Option<String>,
    #[serde(default)]
    pub concept_key: Option<String>,
}

impl StudyStep {
    pub(crate) fn memory_key(&self, topic: &str) -> String {
        self.concept_key
            .clone()
            .or_else(|| self.card_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}:{}",
                    topic,
                    self.title
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect::<String>()
                )
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyReviewTarget {
    pub concept_key: String,
    pub topic: String,
    pub title: String,
    pub previous_question: String,
    pub previous_quiz: StudyQuiz,
}

/// Snapshot the explicitly chosen card, without unrelated card history or user state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyCardTarget {
    pub card_id: String,
    pub title: String,
    pub topic: String,
    pub short_answer: String,
    pub explanation: String,
    pub trust_status: crate::models::TrustStatus,
    pub source_refs: Vec<crate::models::SourceRef>,
}

impl From<crate::models::KnowledgeCard> for StudyCardTarget {
    fn from(card: crate::models::KnowledgeCard) -> Self {
        Self {
            card_id: card.id,
            title: card.question,
            topic: card.topic_label,
            short_answer: card.short_answer,
            explanation: card.explanation,
            trust_status: card.trust_status,
            source_refs: card.source_refs,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudyCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StudySession {
    pub id: String,
    pub goal: String,
    pub topic: String,
    pub provider: String,
    pub region: String,
    pub model: String,
    pub prompt_version: String,
    pub state: String,
    pub revision: i64,
    pub created_at: String,
    pub steps: Vec<StudyStep>,
    pub messages: Vec<Value>,
    pub pending: Option<StudyCall>,
    pub discovered_cards: Vec<String>,
    pub context_read: bool,
    pub searched: bool,
    pub next_topic: Option<String>,
    pub error: Option<String>,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub control: RunControl,
    #[serde(default)]
    pub review_target: Option<StudyReviewTarget>,
    #[serde(default)]
    pub source_card: Option<StudyCardTarget>,
    #[serde(default)]
    pub source_expanded: bool,
    #[serde(default)]
    pub questions: Vec<StudyQuestion>,
    #[serde(default)]
    pub doubt_target: Option<StudyDoubtTarget>,
    #[serde(default)]
    pub goal_mode: bool,
    #[serde(default)]
    pub goal_plan: Option<crate::study_goal::StudyPlan>,
}

impl StudySession {
    pub(crate) fn pending_question(&self) -> Option<&StudyQuestion> {
        self.questions.last().filter(|q| q.answer.is_none())
    }

    pub(crate) fn can_ask(&self) -> bool {
        ["waiting", "ready"].contains(&self.state.as_str())
            && self.pending.is_none()
            && self.pending_question().is_none()
            && self.questions.len() < MAX_QUESTIONS
            && self
                .steps
                .last()
                .is_some_and(|s| s.quiz.is_none() || s.feedback.as_deref() == Some("answer"))
            && self.control.stop.as_ref().map_or(true, |s| s.resumable)
            && self.model_calls < self.control.policy.max_model_calls
            && self.tool_calls < self.control.policy.max_tool_calls
            && self.control.charged_tokens < self.control.policy.max_token_charge
            && self.control.charged_active_ms < self.control.policy.max_active_ms
    }
    // Display only reasons supported by recorded actions, including for older sessions.
    // A model-provided reason may mistake “continue” for evidence of understanding.
    fn step_reason(&self, index: usize) -> &'static str {
        let step = &self.steps[index];
        if step.quiz.is_some() {
            return "用一道可跳过的小题回顾刚才的内容，提交后才记录练习结果。";
        }
        let previous = index.checked_sub(1).and_then(|i| self.steps.get(i));
        match previous.and_then(|s| s.feedback.as_deref()) {
            Some("confused") => "你选择了“没看懂”，这一步先补基础或换个例子。",
            Some("example") => "你想看一个例子，这一步用具体情境来说明。",
            Some("easy") => "你觉得上一段太简单，这一步继续展开。",
            Some("answer")
                if previous
                    .and_then(|s| s.quiz.as_ref())
                    .is_some_and(|q| q.selected.is_some_and(|s| s != q.correct_index)) =>
            {
                "上一题与参考答案不一致，这一步回顾相关知识。"
            }
            _ => "围绕这次的学习主题，继续看一小段内容。",
        }
    }

    pub fn new(
        goal: String,
        topic: String,
        provider: &str,
        region: &str,
        context: &ProviderContext,
    ) -> Self {
        let mut control = RunControl::default();
        control.policy.max_model_calls = 24;
        control.policy.max_tool_calls = 36;
        control.policy.max_token_charge = 600_000;
        control.policy.max_active_ms = 600_000;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            goal,
            topic,
            provider: provider.into(),
            region: region.into(),
            model: context.model.clone(),
            prompt_version: PROMPT_VERSION.into(),
            state: "ready".into(),
            revision: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
            steps: vec![],
            messages: vec![],
            pending: None,
            discovered_cards: vec![],
            context_read: false,
            searched: false,
            next_topic: None,
            error: None,
            model_calls: 0,
            tool_calls: 0,
            control,
            review_target: None,
            source_card: None,
            source_expanded: false,
            questions: vec![],
            doubt_target: None,
            goal_mode: false,
            goal_plan: None,
        }
    }

    pub fn public(&self) -> Value {
        let answered: Vec<_> = self
            .steps
            .iter()
            .filter_map(|s| s.quiz.as_ref())
            .filter(|q| q.selected.is_some())
            .collect();
        json!({
            "id":self.id,"goal":self.goal,"topic":self.topic,"state":self.state,
            "revision":self.revision,"created_at":self.created_at,"provider":self.provider,"model":self.model,
            "error":self.error,"next_topic":self.next_topic,
            "questions":self.questions,"can_ask":self.can_ask(),"question_limit":MAX_QUESTIONS,
            "doubt_target":self.doubt_target,
            "goal_mode":self.goal_mode,"goal_plan":self.goal_public(),
            "can_finish_goal":self.goal_mode && self.goal_finished(),
            "can_reopen_goal":self.goal_mode && self.state == "completed" && !self.goal_finished() && self.goal_has_budget(),
            "source_card":self.source_card.as_ref().map(|c|json!({"card_id":c.card_id,"title":c.title})),
            "source_expanded":self.source_expanded,
            "review_target":self.review_target.as_ref().map(|r|json!({"concept_key":r.concept_key,"title":r.title})),
            "can_resume": (["ready","paused","failed"].contains(&self.state.as_str())
                && self.control.stop.as_ref().map_or(true, |s| s.resumable)
                && (self.model_calls < self.control.policy.max_model_calls || self.pending.is_some())
                && self.tool_calls < self.control.policy.max_tool_calls
                && self.control.charged_tokens < self.control.policy.max_token_charge
                && self.control.charged_active_ms < self.control.policy.max_active_ms),
            "steps":self.steps.iter().enumerate().map(|(index,s)| json!({
                "id":s.id,"kind":s.kind,"title":s.title,"text":s.text,"reason":self.goal_step_reason(s).unwrap_or_else(||self.step_reason(index).into()),
                "card_id":s.card_id,"feedback":s.feedback,
                "quiz":s.quiz.as_ref().map(|q| json!({"options":q.options,"selected":q.selected,
                    "correct_index":q.selected.map(|_|q.correct_index),
                    "explanation":q.selected.map(|_|&q.explanation),
                    "correct":q.selected.map(|selected|selected==q.correct_index)}))
            })).collect::<Vec<_>>(),
            "summary":{"topics":self.steps.iter().filter(|s|s.kind!="quiz").map(|s|&s.title).collect::<Vec<_>>(),
                "answered":answered.len(),"correct":answered.iter().filter(|q|q.selected==Some(q.correct_index)).count()}
        })
    }
}
