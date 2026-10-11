//! Append-only JSONL caches: one line per answer, written as it arrives, so a
//! killed run loses at most the answer in flight.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Every line of `path`, or nothing if it doesn't exist yet. A last line cut
/// off by a kill is dropped with a warning; a bad line anywhere else is an
/// error, since the file was written by this tool.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut rows = Vec::with_capacity(lines.len());
    for (n, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(row) => rows.push(row),
            Err(_) if n + 1 == lines.len() && !text.ends_with('\n') => {
                tracing::warn!("{}: dropping a last line cut short", path.display());
            }
            Err(e) => {
                return Err(e).with_context(|| format!("{} line {}", path.display(), n + 1));
            }
        }
    }
    Ok(rows)
}

pub struct Appender(File);

impl Appender {
    pub fn open(path: &Path) -> Result<Appender> {
        // A line cut short by a kill goes, or the next would be glued to it.
        if let Ok(text) = std::fs::read(path)
            && !text.is_empty()
            && !text.ends_with(b"\n")
        {
            let keep = text.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            OpenOptions::new()
                .write(true)
                .open(path)?
                .set_len(keep as u64)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        Ok(Appender(file))
    }

    pub fn push<T: Serialize>(&mut self, row: &T) -> Result<()> {
        let mut line = serde_json::to_string(row)?;
        line.push('\n');
        self.0.write_all(line.as_bytes())?;
        self.0.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_cut_short_is_dropped_and_appending_carries_on() {
        let dir = std::env::temp_dir().join(format!("jsonl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.jsonl");
        std::fs::write(&path, "[1]\n[2]\n[3").unwrap();
        assert_eq!(read::<Vec<i32>>(&path).unwrap(), [vec![1], vec![2]]);

        let mut out = Appender::open(&path).unwrap();
        out.push(&vec![4]).unwrap();
        assert_eq!(
            read::<Vec<i32>>(&path).unwrap(),
            [vec![1], vec![2], vec![4]]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
