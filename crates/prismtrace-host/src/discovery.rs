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
            .args(["-axo", "pid=,comm=,args="])
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

    let pid_end = trimmed.find(char::is_whitespace)?;
    let pid = trimmed[..pid_end].parse().ok()?;
    let rest = trimmed[pid_end..].trim_start();
    let executable_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let executable_path = &rest[..executable_end];
    let command_line = rest[executable_end..].trim_start();

    Some(ProcessSample {
        pid,
        process_name: executable_path
            .rsplit('/')
            .next()
            .unwrap_or(executable_path)
            .to_string(),
        executable_path: executable_path.into(),
        command_line: (!command_line.is_empty()).then(|| command_line.to_string()),
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
            command_line: None,
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
            command_line: None,
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
        assert_eq!(sample.command_line, None);
    }

    #[test]
    fn parse_ps_line_preserves_command_line_for_node_helpers() {
        let sample = parse_ps_line(
            "  345 /usr/local/bin/node node /Users/huaxin/.cache/opencode/packages/yaml-language-server/node_modules/.bin/yaml-language-server --stdio",
        )
        .unwrap();

        assert_eq!(sample.pid, 345);
        assert_eq!(sample.process_name, "node");
        assert_eq!(sample.executable_path, PathBuf::from("/usr/local/bin/node"));
        assert_eq!(
            sample.command_line,
            Some(
                "node /Users/huaxin/.cache/opencode/packages/yaml-language-server/node_modules/.bin/yaml-language-server --stdio"
                    .into()
            )
        );
    }
}
