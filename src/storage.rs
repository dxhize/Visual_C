//! Ordered recoverable JSONL journal. Each complete line is independently parseable.
use crate::error::Result;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub struct Journal {
    file: Option<File>,
    pub bytes: u64,
    cap: usize,
}
impl Journal {
    pub fn new(path: Option<&Path>, cap: usize) -> Result<Self> {
        // Never overwrite an existing trace or user file.
        let file = path
            .map(|p| OpenOptions::new().write(true).create_new(true).open(p))
            .transpose()?;
        Ok(Self {
            file,
            bytes: 0,
            cap,
        })
    }
    pub fn append(&mut self, kind: &str, value: &impl Serialize) -> Result<usize> {
        #[derive(Serialize)]
        struct Record<'a, T> {
            kind: &'a str,
            data: &'a T,
        }
        let record = Record { kind, data: value };
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        if self.bytes.saturating_add(bytes.len() as u64) > self.cap as u64 {
            return Err(crate::error::EngineError::LimitReached("max_trace_bytes"));
        }
        if let Some(file) = &mut self.file {
            file.write_all(&bytes)?;
            // File (not BufWriter): completed records are visible even on interruption.
        }
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    pub fn finalize(&mut self) -> Result<()> {
        // Finalization footer is control metadata, allowed beyond the data budget.
        if let Some(file) = &mut self.file {
            file.write_all(b"{\"kind\":\"finalized\",\"data\":true}\n")?;
            file.sync_data()?;
        }
        Ok(())
    }
}

/// Recover complete records incrementally; a crash-torn last line is ignored.
pub fn recover(path: &Path) -> Result<impl Iterator<Item = Result<serde_json::Value>>> {
    use std::io::{BufRead, Read};
    let mut reader = std::io::BufReader::new(File::open(path)?);
    Ok(std::iter::from_fn(move || {
        let mut line = Vec::new();
        let result = (&mut reader)
            .take(16 * 1024 * 1024)
            .read_until(b'\n', &mut line);
        match result {
            Ok(0) => None,
            Ok(_) if line.last() != Some(&b'\n') => None,
            Ok(_) => Some(serde_json::from_slice(&line).map_err(Into::into)),
            Err(e) => Some(Err(e.into())),
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_no_overwrite() {
        let mut j = Journal::new(None, 60).unwrap();
        j.append("test", &1).unwrap();
        assert!(j.append("large", &"x".repeat(80)).is_err());
    }

    #[test]
    fn torn_last_record_is_recoverable_and_existing_file_is_not_overwritten() {
        let path = std::env::temp_dir().join(format!("cte-storage-{}.jsonl", std::process::id()));
        let mut journal = Journal::new(Some(&path), 1000).unwrap();
        journal.append("instruction", &1).unwrap();
        drop(journal);
        assert!(Journal::new(Some(&path), 1000).is_err());
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"kind\":\"torn").unwrap();
        drop(file);
        let records: Vec<_> = recover(&path).unwrap().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["data"], 1);
        std::fs::remove_file(path).unwrap();
    }
}
