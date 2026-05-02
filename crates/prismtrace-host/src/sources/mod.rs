pub use prismtrace_sources::{
    ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
    ObserverChannelKind, ObserverHandshake, ObserverSession, ObserverSource, ObserverSourceFactory,
};

pub mod claude {
    pub use crate::claude_observer::{
        ClaudeObserverOptions, default_transcript_root, run_claude_observer,
    };
}

pub mod codex {
    pub use crate::codex_observer::{CodexObserverOptions, run_codex_observer};
}

pub mod opencode {
    pub use crate::opencode_observer::{OpencodeObserverOptions, run_opencode_observer};
}
