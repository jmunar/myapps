//! Stage `solve`: a stronger model actually attempts each problem, and its
//! answer is checked against the dataset's. How often a problem defeats the
//! solver is a difficulty that needs no expert, so `rank` can say whether the
//! levels, the prior or the grade predict it best (UGPhysics' levels are a
//! skill ladder, and the judge disagreed with them).
//!
//! The solver thinks before answering, so this is minutes a problem rather
//! than seconds: meant for a few hundred, with `--limit`. The check is the
//! same model with its thinking off, one token, unless the answer already
//! matches by rule.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::flags::progress;
use super::jsonl;
use super::llm::Llm;
use super::rubric;
use crate::grades::text_hash;
use crate::source::last_boxed;
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

/// The thinking budget the server is started with, in tokens. A problem the
/// solver cannot finish thinking about in this many still gets an answer,
/// forced, and usually a wrong one: past this, "harder" is all we need.
pub const THINKING: u32 = 8192;
/// The backstop on the whole reply: the budget plus room for the write-up.
const MAX_TOKENS: u32 = THINKING + 4096;

/// What the server needs beyond the model: room for the problem and the
/// reply, and the thinking budget.
pub fn server_options() -> String {
    format!(
        "-c 16384 --reasoning-budget {THINKING} \
         --reasoning-budget-message \"Time is up, so I'll give my final answer now.\""
    )
}

/// One attempt at one problem.
#[derive(Serialize, Deserialize)]
pub struct Attempt {
    pub source_key: String,
    pub text_hash: String,
    /// 0 for the first; each has its own sampling seed.
    pub attempt: u64,
    /// The last `\boxed{}` in the reply, if it gave one.
    pub answer: Option<String>,
    /// The probability the answer is right: 1 or 0 by rule, else the
    /// checker's P(yes).
    pub p_correct: f64,
    /// "rule", "model", or "none" when there was no answer to check.
    pub checked_by: String,
    /// The server's finish reason; "length" means cut off by `max_tokens`.
    pub finish: String,
    /// Tokens generated, thinking included: how long it had to think.
    pub tokens: u64,
    pub seconds: f64,
    /// The reply after the thinking, to look into what went wrong.
    pub reply: String,
}

fn path(dir: &Path) -> PathBuf {
    dir.join("solve.jsonl")
}

pub fn load(dir: &Path) -> Result<Vec<Attempt>> {
    jsonl::read(&path(dir))
}

pub async fn run(
    llm: &Llm,
    dataset: Dataset,
    problems: &[&Problem],
    attempts: u64,
    dir: &Path,
) -> Result<()> {
    let done: HashSet<(String, String, u64)> = load(dir)?
        .into_iter()
        .map(|a| (a.source_key, a.text_hash, a.attempt))
        .collect();
    // Every problem's first attempt before any second one, so a run stopped
    // early still covers the problems evenly.
    let todo: Vec<(&Problem, String, u64)> = (0..attempts)
        .flat_map(|n| problems.iter().map(move |p| (*p, n)))
        .map(|(p, n)| (p, text_hash(&p.problem), n))
        .filter(|(p, hash, n)| !done.contains(&(p.source_key.clone(), hash.clone(), *n)))
        .collect();
    tracing::info!(
        "solve: {} attempts to go over {} problems, {attempts} each ({})",
        todo.len(),
        problems.len(),
        path(dir).display()
    );

    let system = format!(
        "You are an expert in {}. Solve the problem, then give the final answer in \\boxed{{}}.",
        rubric::field(dataset)
    );
    let mut out = jsonl::Appender::open(&path(dir))?;
    let (start, mut failed, mut right) = (Instant::now(), 0, 0.0);
    for (n, (p, hash, attempt)) in todo.iter().enumerate() {
        let began = Instant::now();
        let result = async {
            let reply = llm
                .generate(&system, &prompt(p), MAX_TOKENS, *attempt)
                .await?;
            let answer = last_boxed(&reply.content).map(|a| a.trim().to_string());
            let (p_correct, checked_by) = match &answer {
                None => (0.0, "none"),
                Some(a) => match by_rule(a, reference(p)) {
                    true => (1.0, "rule"),
                    false => (check(llm, dataset, p, a).await?, "model"),
                },
            };
            anyhow::Ok(Attempt {
                source_key: p.source_key.clone(),
                text_hash: hash.clone(),
                attempt: *attempt,
                answer,
                p_correct,
                checked_by: checked_by.to_string(),
                finish: reply.finish,
                tokens: reply.tokens,
                seconds: began.elapsed().as_secs_f64(),
                reply: reply.content,
            })
        }
        .await;
        match result {
            Ok(a) => {
                right += a.p_correct;
                tracing::info!(
                    "solve: {} #{}: {} tokens, {:.0} s, {} → {:.2} ({}); {:.0}% right so far",
                    a.source_key,
                    a.attempt,
                    a.tokens,
                    a.seconds,
                    a.answer.as_deref().unwrap_or("no answer"),
                    a.p_correct,
                    a.checked_by,
                    100.0 * right / (n + 1 - failed) as f64
                );
                out.push(&a)?;
            }
            Err(e) => {
                tracing::warn!("solve: {} skipped: {e:#}", p.source_key);
                failed += 1;
            }
        }
        progress("solve", n + 1, todo.len(), start, 10);
    }
    tracing::info!("solve: done, {failed} skipped");
    Ok(())
}

fn prompt(p: &Problem) -> String {
    let mut text = format!(
        "{}\n\nPut the final answer in \\boxed{{}}. If the problem asks for several \
         answers, put them all in one \\boxed{{}}, separated by commas, in the order asked.",
        p.problem
    );
    if let Some(unit) = p.unit.as_deref().and_then(units) {
        text.push_str(&format!(
            " Give the answers in these units, in order: {unit}."
        ));
    }
    text
}

/// UGPhysics' units, readable, or `None` when every answer is unitless.
fn units(unit: &str) -> Option<String> {
    let plain = unit
        .replace("\\text{", "")
        .replace("\\mathrm{", "")
        .replace('}', "");
    let parts: Vec<&str> = plain.split(',').map(str::trim).collect();
    if parts
        .iter()
        .all(|u| u.is_empty() || u.eq_ignore_ascii_case("none"))
    {
        return None;
    }
    let parts: Vec<&str> = parts
        .iter()
        .map(|u| {
            if u.eq_ignore_ascii_case("none") {
                "none"
            } else {
                u
            }
        })
        .collect();
    Some(parts.join(", "))
}

/// The dataset's answer without its box: UGPhysics boxes its answers,
/// Hendrycks' were taken out of theirs when mapped.
fn reference(p: &Problem) -> &str {
    last_boxed(&p.answer).unwrap_or(&p.answer).trim()
}

/// Right without asking: the same text once spacing and sizing are gone, or
/// the same number to 1%. Anything else, such as an equivalent expression or
/// the same value in other units, goes to the checker.
fn by_rule(answer: &str, reference: &str) -> bool {
    let (a, r) = (normalise(answer), normalise(reference));
    if a == r {
        return true;
    }
    match (number(&a), number(&r)) {
        (Some(a), Some(r)) => (a - r).abs() <= 0.01 * a.abs().max(r.abs()),
        _ => false,
    }
}

fn normalise(s: &str) -> String {
    let mut s = s.to_string();
    for (from, to) in [
        ("\\left", ""),
        ("\\right", ""),
        ("\\dfrac", "\\frac"),
        ("\\tfrac", "\\frac"),
        ("\\,", ""),
        ("\\;", ""),
        ("\\!", ""),
        ("$", ""),
    ] {
        s = s.replace(from, to);
    }
    s.retain(|c| !c.is_whitespace());
    s.trim_end_matches('.').to_string()
}

/// A plain number, possibly in scientific notation and followed by a unit in
/// `\text{}` or `\mathrm{}`, from a normalised answer.
fn number(s: &str) -> Option<f64> {
    let mut s = s.to_string();
    for unit in ["\\text{", "\\mathrm{"] {
        if let Some(i) = s.find(unit)
            && s.ends_with('}')
        {
            s.truncate(i);
        }
    }
    for times in ["\\times10^", "\\cdot10^", "×10^"] {
        s = s.replace(times, "e");
    }
    s.retain(|c| c != '{' && c != '}');
    s.parse().ok().filter(|x: &f64| x.is_finite())
}

/// The checker: the same model, thinking off, asked whether the answer is
/// the reference's in another form.
async fn check(llm: &Llm, dataset: Dataset, p: &Problem, answer: &str) -> Result<f64> {
    let system = format!(
        "You check answers to {} problems. Answer with a single letter.",
        rubric::field(dataset)
    );
    let unit = match p.unit.as_deref().and_then(units) {
        Some(unit) => format!(" (units: {unit})"),
        None => String::new(),
    };
    let user = format!(
        "Problem:\n{}\n\nReference answer: {}{unit}\n\nProposed answer: {answer}\n\n\
         Is the proposed answer equivalent to the reference answer? The same value in \
         another form, in other units or rounded differently counts as equivalent; a \
         different value, a missing part or a wrong sign does not.\n\
         A. yes\nB. no\n\nAnswer with the letter only.",
        p.problem,
        reference(p)
    );
    let (probs, _) = llm.choose(&system, &user, 2).await?;
    Ok(probs[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_matches_spacing_sizing_and_close_numbers() {
        assert!(by_rule("\\left(1, 2\\right)", "(1,2)"));
        assert!(by_rule("\\dfrac{1}{2}", "\\frac{1}{2}"));
        assert!(by_rule("0.786", "0.79"));
        assert!(by_rule("5.0 \\times 10^{3} \\text{ m}", "5000"));
        assert!(by_rule("Yes.", "Yes"));
        assert!(!by_rule("0.70", "0.79"));
        assert!(!by_rule("-5000", "5000"));
        assert!(!by_rule("\\frac{2}{4}", "\\frac{1}{2}"));
    }

    #[test]
    fn units_drop_when_every_answer_is_unitless() {
        assert_eq!(units("None, None"), None);
        assert_eq!(units("\\text{None, None}"), None);
        assert_eq!(units("\\mathrm{MeV}").as_deref(), Some("MeV"));
        assert_eq!(units("None, \\mathrm{m}").as_deref(), Some("none, m"));
    }

    #[test]
    fn the_reference_comes_out_of_its_box() {
        let mut p = Problem {
            source_key: "k".into(),
            subject: "s".into(),
            topic: None,
            difficulty: 1,
            source_level: "1".into(),
            problem: "p".into(),
            solution: "s".into(),
            answer: "\\boxed{\\frac{1}{2}}".into(),
            answer_type: None,
            unit: None,
        };
        assert_eq!(reference(&p), "\\frac{1}{2}");
        p.answer = "(18, -18)".into();
        assert_eq!(reference(&p), "(18, -18)");
    }
}
