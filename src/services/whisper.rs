use std::path::Path;
use tokio::process::Command;

use crate::config::Config;

/// Convert audio to 16 kHz mono WAV (required by whisper.cpp).
/// Returns the path to the converted file.
pub async fn convert_to_wav(input: &Path) -> anyhow::Result<std::path::PathBuf> {
    let output = input.with_extension("16k.wav");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input.to_str().unwrap_or(""),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            output.to_str().unwrap_or(""),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("ffmpeg conversion failed with exit code: {}", status);
    }
    Ok(output)
}

/// Run whisper.cpp CLI on a 16 kHz WAV file. Returns the transcribed text.
pub async fn transcribe(config: &Config, wav_path: &Path, model: &str) -> anyhow::Result<String> {
    let whisper_bin = &config.whisper_cli_path;
    let model_path = config.whisper_model_path(model);

    let output = Command::new(whisper_bin)
        .args([
            "-m",
            &model_path,
            "-f",
            wav_path.to_str().unwrap_or(""),
            "--no-timestamps",
            "-l",
            "auto",
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("whisper-cli failed: {stderr}");
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(text)
}
