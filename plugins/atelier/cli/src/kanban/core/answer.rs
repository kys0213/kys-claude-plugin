//! A person's picks on the kanban → the answers an AskUserQuestion call
//! returns, one per question in question order.

use super::question::Question;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub options: Vec<String>,
    pub text: Option<String>,
}

pub fn compose(questions: &[Question], picks: &[Pick]) -> Result<Vec<String>, String> {
    if questions.len() != picks.len() {
        return Err(format!(
            "질문 {}개에 답 {}개가 왔어요",
            questions.len(),
            picks.len()
        ));
    }
    questions
        .iter()
        .zip(picks)
        .map(|(question, pick)| answer_one(question, pick))
        .collect()
}

fn answer_one(question: &Question, pick: &Pick) -> Result<String, String> {
    let typed = pick.text.as_deref().map(str::trim).unwrap_or("");
    if !typed.is_empty() {
        return Ok(typed.to_string());
    }
    if pick.options.is_empty() {
        return Err(format!("답하지 않은 질문이 있어요: {}", question.header));
    }
    if let Some(unknown) = pick
        .options
        .iter()
        .find(|label| !question.options.iter().any(|o| &o.label == *label))
    {
        return Err(format!("없는 선택지예요: {} — {unknown}", question.header));
    }
    if !question.multi_select && pick.options.len() > 1 {
        return Err(format!("하나만 고를 수 있어요: {}", question.header));
    }
    Ok(question
        .options
        .iter()
        .filter(|o| pick.options.contains(&o.label))
        .map(|o| o.label.as_str())
        .collect::<Vec<_>>()
        .join(", "))
}
