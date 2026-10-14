//! The model the grader talks to: a `llama-server` chat endpoint, asked for a
//! single token and read through its top log-probabilities, so every answer is
//! a probability over lettered options rather than sampled text. `solve` also
//! lets it think and write a solution.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const LETTERS: [&str; 4] = ["A", "B", "C", "D"];

pub struct Llm {
    client: reqwest::Client,
    base: String,
}

impl Llm {
    /// Check that `base` is a llama-server with `model` loaded. The server is
    /// started by hand, so how much memory it takes stays the user's call;
    /// when there is none, this prints the command to start one, with the
    /// stage's own `options` (context size and the like).
    pub async fn connect(base: &str, model: &str, options: &str) -> Result<Llm> {
        let base = base.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;
        let props: Value = match client.get(format!("{base}/props")).send().await {
            Ok(response) => response.json().await.context("reading /props")?,
            Err(e) => bail!(
                "no llama-server at {base} ({e}). Start one, alone on the GPU:\n  \
                 llama-server -m ~/Models/{model} -ngl 99 {options} --parallel 1 --port 8091"
            ),
        };
        let path = props["model_path"].as_str().unwrap_or_default();
        let loaded = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        if loaded != model {
            bail!("llama-server at {base} has '{loaded}' loaded, not '{model}' (see --model)");
        }
        Ok(Llm { client, base })
    }

    /// The probability of each of the first `options` letters as the answer,
    /// normalised over them, and the probability mass they had before that
    /// (low means the model wanted to say something else).
    pub async fn choose(
        &self,
        system: &str,
        user: &str,
        options: usize,
    ) -> Result<(Vec<f64>, f64)> {
        let body = json!({
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "max_tokens": 1,
            "temperature": 0,
            "logprobs": true,
            "top_logprobs": 20,
            "chat_template_kwargs": {"enable_thinking": false},
        });
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let reply: Value = response.json().await?;
        if !status.is_success() {
            bail!("llama-server answered {status}: {reply}");
        }
        let top = reply["choices"][0]["logprobs"]["content"][0]["top_logprobs"]
            .as_array()
            .context("no top_logprobs in the reply")?;
        Ok(letter_probabilities(top, options))
    }

    /// A thought-out reply, sampled with the model's own settings for
    /// thinking (greedy decoding makes it loop). `seed` makes a repeated
    /// attempt differ from the first and still be reproducible. How long it
    /// may think is the server's `--reasoning-budget`; `max_tokens` is the
    /// backstop.
    pub async fn generate(
        &self,
        system: &str,
        user: &str,
        sampling: Sampling,
        max_tokens: u32,
        seed: u64,
    ) -> Result<Reply> {
        let body = json!({
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "max_tokens": max_tokens,
            "temperature": sampling.temperature,
            "top_p": sampling.top_p,
            "top_k": sampling.top_k,
            "min_p": 0,
            "seed": seed,
            "chat_template_kwargs": {"enable_thinking": true},
        });
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base))
            .timeout(Duration::from_secs(3 * 3600))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let reply: Value = response.json().await?;
        if !status.is_success() {
            bail!("llama-server answered {status}: {reply}");
        }
        let choice = &reply["choices"][0];
        Ok(Reply {
            content: choice["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            finish: choice["finish_reason"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            tokens: reply["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        })
    }
}

#[derive(Clone, Copy)]
pub struct Sampling {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
}

impl Sampling {
    /// What each family's model card recommends for thinking: Gemma 4's, or
    /// Qwen's (the precise-task temperature) for anything else.
    pub fn for_model(model: &str) -> Sampling {
        if model.to_ascii_lowercase().starts_with("gemma") {
            Sampling {
                temperature: 1.0,
                top_p: 0.95,
                top_k: 64,
            }
        } else {
            Sampling {
                temperature: 0.6,
                top_p: 0.95,
                top_k: 20,
            }
        }
    }
}

pub struct Reply {
    /// The reply after the thinking, which the server keeps apart.
    pub content: String,
    /// "stop", or "length" when `max_tokens` cut it off.
    pub finish: String,
    /// Tokens generated, the thinking included.
    pub tokens: u64,
}

/// "A" and " A" are the same answer: add them up rather than letting one
/// overwrite the other (the bug that once made the 9B judge look like 70%).
fn letter_probabilities(top: &[Value], options: usize) -> (Vec<f64>, f64) {
    let mut p = vec![0.0; options];
    for t in top {
        let (Some(token), Some(logprob)) = (t["token"].as_str(), t["logprob"].as_f64()) else {
            continue;
        };
        if let Some(i) = LETTERS[..options].iter().position(|l| *l == token.trim()) {
            p[i] += logprob.exp();
        }
    }
    let mass: f64 = p.iter().sum();
    if mass > 0.0 {
        p.iter_mut().for_each(|x| *x /= mass);
    } else {
        p.fill(1.0 / options as f64);
    }
    (p, mass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_a_letter_add_up() {
        let top = [
            json!({"token": "A", "logprob": (0.3f64).ln()}),
            json!({"token": " A", "logprob": (0.3f64).ln()}),
            json!({"token": "B", "logprob": (0.2f64).ln()}),
            json!({"token": "The", "logprob": (0.2f64).ln()}),
        ];
        let (p, mass) = letter_probabilities(&top, 2);
        assert!((mass - 0.8).abs() < 1e-9);
        assert!((p[0] - 0.75).abs() < 1e-9 && (p[1] - 0.25).abs() < 1e-9);
    }
}
