//! The one model the grader talks to: a `llama-server` chat endpoint, asked
//! for a single token and read through its top log-probabilities, so every
//! answer is a probability over lettered options rather than sampled text.

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
    /// this prints the command the spike used when there is none.
    pub async fn connect(base: &str, model: &str) -> Result<Llm> {
        let base = base.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;
        let props: Value = match client.get(format!("{base}/props")).send().await {
            Ok(response) => response.json().await.context("reading /props")?,
            Err(e) => bail!(
                "no llama-server at {base} ({e}). Start one, alone on the GPU:\n  \
                 llama-server -m ~/Models/{model} -ngl 99 -c 8192 --parallel 1 --port 8091"
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
