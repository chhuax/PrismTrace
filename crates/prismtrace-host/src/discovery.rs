use prismtrace_core::{ProcessSample, ProcessTarget};
use std::io;
use std::process::Command;

pub trait ProcessSampleSource {
    fn collect_samples(&self) -> io::Result<Vec<ProcessSample>>;
}

#[derive(Debug, Clone)]
pub struct StaticProcessSampleSource {
    samples: Vec<ProcessSample>,
}

impl StaticProcessSampleSource {
    pub fn new(samples: Vec<ProcessSample>) -> Self {
        Self { samples }
    }
}

impl ProcessSampleSource for StaticProcessSampleSource {
    fn collect_samples(&self) -> io::Result<Vec<ProcessSample>> {
        Ok(self.samples.clone())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PsProcessSampleSource;

impl ProcessSampleSource for PsProcessSampleSource {
    fn collect_samples(&self) -> io::Result<Vec<ProcessSample>> {
        let output = Command::new("ps")
            .args(["-axo", "pid=,comm="])
            .output()?;

        if !output.status.success() {
            return Err(io::Error::other("ps command failed"));
        }

        Ok(parse_ps_output(&String::from_utf8_lossy(&output.stdout)))
    }
}

pub fn discover_targets(source: &impl ProcessSampleSource) -> io::Result<Vec<ProcessTarget>> {
    let samples = source.collect_samples()?;

    Ok(samples.iter().map(ProcessSample::into_target).collect())
}

pub fn discover_current_process_targets() -> io::Result<Vec<ProcessTarget>> {
    discover_targets(&PsProcessSampleSource)
}

fn parse_ps_output(output: &str) -> Vec<ProcessSample> {
    output.lines().filter_map(parse_ps_line).collect()
}

fn parse_ps_line(line: &str) -> Option<ProcessSample> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let pid = parts.next()?.parse().ok()?;
    let executable_path = parts.next()?;

    Some(ProcessSample {
        pid,
        process_name: executable_path
            .rsplit('/')
            .next()
            .unwrap_or(executable_path)
            .to_string(),
        executable_path: executable_path.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::{StaticProcessSampleSource, discover_targets, parse_ps_line};
    use prismtrace_core::{ProcessSample, RuntimeKind};
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn discover_targets_returns_structured_process_targets() -> io::Result<()> {
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 200,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        }]);

        let targets = discover_targets(&source)?;

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].pid, 200);
        assert_eq!(targets[0].runtime_kind, RuntimeKind::Node);
        Ok(())
    }

    #[test]
    fn discover_targets_preserves_unknown_runtime_kind() -> io::Result<()> {
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 201,
            process_name: "python3".into(),
            executable_path: PathBuf::from("/usr/bin/python3"),
        }]);

        let targets = discover_targets(&source)?;

        assert_eq!(targets[0].runtime_kind, RuntimeKind::Unknown);
        Ok(())
    }

    #[test]
    fn parse_ps_line_extracts_pid_and_executable_path() {
        let sample =
            parse_ps_line("  345 /Applications/Electron.app/Contents/MacOS/Electron").unwrap();

        assert_eq!(sample.pid, 345);
        assert_eq!(sample.process_name, "Electron");
        assert_eq!(
            sample.executable_path,
            PathBuf::from("/Applications/Electron.app/Contents/MacOS/Electron")
        );
    }
}
