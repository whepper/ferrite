//! Terminal rendering for the renderer-agnostic batch progress events.
//!
//! This module lives in the binary, not in the conversion library. It consumes
//! [`aac_convert::progress::ProgressEvent`] values and is the only place that
//! knows about terminals, carriage returns, or ANSI erase sequences. A GUI can
//! consume the same events directly.

use std::io::{IsTerminal as _, Write as _};
use std::path::Path;
use std::time::{Duration, Instant};

use aac_convert::progress::{ProgressEvent, ProgressState};

/// Compact live progress display for batch conversion.
pub(crate) struct ProgressDisplay {
    quiet: bool,
    verbose: bool,
    interactive: bool,
    state: ProgressState,
    preparation_started_at: Option<Instant>,
    artwork_started: Option<Instant>,
    loudness_started: Option<Instant>,
    encoding_started: Option<Instant>,
    milestone: Option<usize>,
    last_noninteractive_emit: Option<Instant>,
    line_active: bool,
    preparation_printed: bool,
    encoding_printed: bool,
}

impl ProgressDisplay {
    /// Create a display. `interactive` is enabled only for a non-quiet,
    /// non-verbose stderr terminal; redirected output gets plain milestone
    /// lines instead of carriage-return updates.
    pub(crate) fn new(quiet: bool, verbose: bool) -> Self {
        Self {
            quiet,
            verbose,
            interactive: !quiet && !verbose && std::io::stderr().is_terminal(),
            state: ProgressState::default(),
            preparation_started_at: None,
            artwork_started: None,
            loudness_started: None,
            encoding_started: None,
            milestone: None,
            last_noninteractive_emit: None,
            line_active: false,
            preparation_printed: false,
            encoding_printed: false,
        }
    }

    /// Consume one pipeline progress event.
    pub(crate) fn handle(&mut self, event: &ProgressEvent) {
        if self.quiet {
            return;
        }
        self.state.apply(event);
        match event {
            ProgressEvent::BatchStarted { .. } => {}
            ProgressEvent::PreparationStarted { .. } => self.preparation_started(),
            ProgressEvent::ArtworkStarted {
                albums, directory, ..
            } => self.artwork_started(*albums, directory),
            ProgressEvent::ArtworkFinished {
                source,
                source_path,
                completed_albums,
                ..
            } => self.artwork_finished(*source, source_path.as_deref(), *completed_albums),
            ProgressEvent::LoudnessFinished {
                completed,
                total,
                relative_input,
                outcome,
            } => self.loudness_finished(*completed, *total, relative_input, *outcome),
            ProgressEvent::PreparationFinished {
                prepared,
                failed,
                skipped,
                ..
            } => self.preparation_finished(*prepared, *failed, *skipped),
            ProgressEvent::EncodingStarted { .. } => self.encoding_started(),
            ProgressEvent::FileStarted(item) => {
                if self.verbose {
                    self.plain_line(&format!("  encoding {}", item.relative_input.display()));
                } else if self.interactive {
                    self.encoding_line();
                }
            }
            ProgressEvent::FileConverted { item, .. } => {
                if self.verbose {
                    self.plain_line(&format!("  done     {}", item.relative_input.display()));
                } else {
                    self.encoding_line();
                }
            }
            ProgressEvent::FileSkipped(item) => {
                if self.verbose {
                    self.plain_line(&format!(
                        "  skipped  {} (already exists)",
                        item.relative_input.display()
                    ));
                } else {
                    self.encoding_line();
                }
            }
            ProgressEvent::FileFailed { item, category, .. } => {
                if self.verbose {
                    self.plain_line(&format!(
                        "  failed   {} ({category})",
                        item.relative_input.display()
                    ));
                } else {
                    self.encoding_line();
                }
            }
            ProgressEvent::FilePlanned(item) => {
                if self.verbose {
                    self.plain_line(&format!("  planned  {}", item.relative_input.display()));
                } else {
                    self.encoding_line();
                }
            }
            // `ProgressEvent` is `#[non_exhaustive]`: ignore future variants
            // so a new pipeline event cannot break the CLI renderer.
            _ => {}
        }
    }

    /// Print a diagnostic (warning or error) without corrupting a live line.
    ///
    /// Diagnostics always print, including under `--quiet`. Write failures
    /// (including broken pipes) are ignored so progress output can never
    /// crash an otherwise successful batch; the batch result and exit code
    /// remain authoritative.
    pub(crate) fn diagnostic(&mut self, message: &str) {
        self.end_line();
        write_stderr_line(message);
        // Intentionally do not reset the non-interactive throttle here:
        // warnings must not cause extra progress lines in redirected output.
    }

    /// Finish the display, terminating any active live line.
    pub(crate) fn finish(&mut self) {
        self.end_line();
    }

    fn preparation_started(&mut self) {
        self.preparation_started_at = Some(Instant::now());
        let files = self.state.files_discovered;
        let albums = self.state.albums_total;
        self.plain_line(&format!(
            "Preparing {files} {} across {albums} {}",
            plural(files, "file", "files"),
            plural(albums, "album", "albums")
        ));
        self.plain_line("");
        self.plain_line("Preparation");
        self.preparation_printed = true;
        self.milestone = None;
        self.last_noninteractive_emit = None;
    }

    fn artwork_started(&mut self, albums: usize, directory: &Path) {
        if self.artwork_started.is_none() {
            self.artwork_started = Some(Instant::now());
            self.milestone = None;
        }
        if self.interactive {
            let completed = self.state.artwork_completed;
            self.live_line(&format!(
                "  Artwork discovery    {completed}/{albums}  {}%  {}",
                percent(completed, albums),
                display_directory(directory)
            ));
        }
    }

    fn artwork_finished(
        &mut self,
        source: aac_convert::progress::ArtworkSource,
        source_path: Option<&Path>,
        completed: usize,
    ) {
        let total = self.state.artwork_total;
        if self.interactive {
            self.live_line(&format!(
                "  Artwork discovery    {completed}/{total}  {}%  ({}, {})",
                percent(completed, total),
                self.state.artwork_reused,
                self.state.artwork_extracted,
            ));
            if completed >= total {
                self.end_line();
            }
        } else if self.should_emit(completed, total) {
            let detail = match source {
                aac_convert::progress::ArtworkSource::ExistingFile => source_path
                    .map(|path| {
                        format!(
                            ", reused {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                    })
                    .unwrap_or_default(),
                aac_convert::progress::ArtworkSource::Embedded => ", extracted".to_string(),
                aac_convert::progress::ArtworkSource::None => String::new(),
            };
            self.plain_line(&format!(
                "  Artwork discovery    {completed}/{total}  {}%{detail}",
                percent(completed, total)
            ));
        }
    }

    fn loudness_finished(
        &mut self,
        completed: usize,
        total: usize,
        relative_input: &Path,
        outcome: aac_convert::progress::LoudnessOutcome,
    ) {
        if self.loudness_started.is_none() {
            self.loudness_started = Some(Instant::now());
            self.milestone = None;
        }
        let suffix = match outcome {
            aac_convert::progress::LoudnessOutcome::Analyzed => String::new(),
            aac_convert::progress::LoudnessOutcome::Unmeasurable => {
                "  (no measurable signal)".to_string()
            }
            aac_convert::progress::LoudnessOutcome::Failed => "  (analysis failed)".to_string(),
        };
        if self.interactive {
            let eta = eta(completed, total, self.loudness_started);
            let eta = eta.map_or_else(String::new, |eta| format!("  eta {eta}"));
            self.live_line(&format!(
                "  Loudness analysis    {completed}/{total}  {}%{eta}  {}{suffix}",
                percent(completed, total),
                relative_input.display()
            ));
            if completed >= total {
                self.end_line();
            }
        } else if self.should_emit(completed, total) {
            self.plain_line(&format!(
                "  Loudness analysis    {completed}/{total}  {}%{suffix}",
                percent(completed, total)
            ));
        }
    }

    fn preparation_finished(&mut self, prepared: usize, failed: usize, skipped: usize) {
        self.end_line();
        let elapsed = elapsed_suffix(self.preparation_started_at);
        self.plain_line(&format!(
            "  Artwork reused       {} {}",
            self.state.artwork_reused,
            plural(self.state.artwork_reused, "album", "albums")
        ));
        self.plain_line(&format!(
            "  Artwork extracted    {} {}",
            self.state.artwork_extracted,
            plural(self.state.artwork_extracted, "album", "albums")
        ));
        self.plain_line(&format!(
            "  Prepared             {prepared} {}{elapsed}",
            plural(prepared, "file", "files")
        ));
        if failed > 0 {
            self.plain_line(&format!(
                "  Preparation failed   {failed} {}",
                plural(failed, "file", "files")
            ));
        }
        if skipped > 0 {
            self.plain_line(&format!(
                "  Preparation skipped  {skipped} {}",
                plural(skipped, "file", "files")
            ));
        }
    }

    fn encoding_started(&mut self) {
        self.end_line();
        if !self.preparation_printed {
            self.plain_line(&format!(
                "Converting {} {} across {} {}",
                self.state.files_discovered,
                plural(self.state.files_discovered, "file", "files"),
                self.state.albums_total,
                plural(self.state.albums_total, "album", "albums")
            ));
        }
        self.plain_line("");
        self.plain_line("Encoding");
        self.encoding_printed = true;
        self.encoding_started = Some(Instant::now());
        self.milestone = None;
        self.last_noninteractive_emit = None;
    }

    fn encoding_line(&mut self) {
        if !self.encoding_printed {
            return;
        }
        let total = self.state.encoding_total;
        let completed = self.state.encoding_completed();
        if total == 0 {
            return;
        }
        let eta = eta(completed, total, self.encoding_started);
        let eta = eta.map_or_else(String::new, |eta| format!("  eta {eta}"));
        let current = self
            .state
            .current_file
            .as_deref()
            .map(|path| format!("  {}", path.display()))
            .unwrap_or_default();
        if self.interactive {
            self.live_line(&format!(
                "  Files                {completed}/{total}  {}%{eta}{current}",
                percent(completed, total)
            ));
            if completed >= total {
                self.end_line();
                self.plain_line(&format!(
                    "  Converted            {} converted, {} skipped, {} failed{}",
                    self.state.converted,
                    self.state.encoding_skipped,
                    self.state.encoding_failed,
                    elapsed_suffix(self.encoding_started)
                ));
            }
        } else if self.should_emit(completed, total) {
            self.plain_line(&format!(
                "  Files                {completed}/{total}  {}%{eta}",
                percent(completed, total)
            ));
            if completed >= total {
                self.plain_line(&format!(
                    "  Converted            {} converted, {} skipped, {} failed{}",
                    self.state.converted,
                    self.state.encoding_skipped,
                    self.state.encoding_failed,
                    elapsed_suffix(self.encoding_started)
                ));
            }
        }
    }

    fn plain_line(&mut self, text: &str) {
        if self.quiet {
            return;
        }
        self.end_line();
        write_stderr_line(text);
    }

    fn live_line(&mut self, text: &str) {
        if !self.interactive || self.quiet {
            return;
        }
        // Keep the live line compact so it does not wrap on narrow
        // terminals: a wrapped line cannot be fully erased with `\r` +
        // erase-line, leaving residue. Truncation is display-only; verbose
        // mode still prints full paths on separate lines.
        let compact = truncate_status(text, MAX_LIVE_LINE_WIDTH);
        let mut stderr = std::io::stderr().lock();
        // Ignore write failures (including broken pipes): progress must
        // never crash the batch.
        let _ = write!(stderr, "\r\x1b[2K{compact}");
        let _ = stderr.flush();
        self.line_active = true;
    }

    fn end_line(&mut self) {
        if self.line_active {
            // Best-effort newline; ignore broken-pipe failures.
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr);
            let _ = stderr.flush();
            self.line_active = false;
        }
    }

    /// Non-interactive throttle: emit at ten-percent milestones, plus a
    /// one-second fallback when milestones are sparse, and always at
    /// completion.
    fn should_emit(&mut self, completed: usize, total: usize) -> bool {
        if completed >= total {
            self.milestone = Some(100);
            self.last_noninteractive_emit = Some(Instant::now());
            return true;
        }
        let percent = percent(completed, total);
        let milestone = percent / 10 * 10;
        let advanced = self.milestone.is_none_or(|last| milestone > last);
        let spaced = self
            .last_noninteractive_emit
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(1));
        if advanced || spaced {
            self.milestone = Some(milestone);
            self.last_noninteractive_emit = Some(Instant::now());
            true
        } else {
            false
        }
    }
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn percent(completed: usize, total: usize) -> usize {
    completed
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or(100)
}

fn eta(completed: usize, total: usize, started: Option<Instant>) -> Option<String> {
    let started = started?;
    if completed == 0 || completed >= total {
        return None;
    }
    let elapsed = started.elapsed().as_secs_f64();
    if elapsed < 0.5 {
        return None;
    }
    let remaining = elapsed / completed as f64 * (total - completed) as f64;
    Some(format_duration(Duration::from_secs_f64(remaining)))
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn elapsed_suffix(started: Option<Instant>) -> String {
    started.map_or_else(String::new, |started| {
        format!(" ({:.2}s)", started.elapsed().as_secs_f64())
    })
}

fn display_directory(directory: &Path) -> String {
    if directory.as_os_str().is_empty() {
        ".".to_string()
    } else {
        directory.display().to_string()
    }
}

/// Maximum width for interactive live status lines.
///
/// Conservative fixed bound (no terminal-width dependency) so wrapped lines
/// cannot leave erase-line residue on 80-column terminals. Longer content is
/// truncated with an ellipsis; this affects only the live display.
const MAX_LIVE_LINE_WIDTH: usize = 100;

/// Truncate a status line to `max` characters (display-only).
///
/// Keeps the leading counter portion intact by truncating from the middle
/// when possible; falls back to tail truncation for short prefixes. Pure
/// function so it can be unit tested without a terminal.
fn truncate_status(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max <= 4 {
        return chars[..max].iter().collect();
    }
    // Prefer to keep the trailing filename visible: keep the head (counters)
    // and the tail (filename), replacing the middle with `...`.
    let keep_tail = (max - 3) / 2;
    let keep_head = max - 3 - keep_tail;
    let head: String = chars[..keep_head].iter().collect();
    let tail: String = chars[chars.len() - keep_tail..].iter().collect();
    format!("{head}...{tail}")
}

/// Best-effort stderr line write; ignores all failures including broken
/// pipes so progress rendering can never crash the batch.
fn write_stderr_line(text: &str) {
    use std::io::Write as _;

    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{text}");
    let _ = stderr.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_truncation_is_bounded_and_preserves_counters() {
        let short = "  Files                1/3  33%";
        assert_eq!(truncate_status(short, 100), short);
        let long = format!("  Files                1/3  33%  {}", "a".repeat(200));
        let truncated = truncate_status(&long, 100);
        assert_eq!(truncated.chars().count(), 100);
        assert!(truncated.contains("Files"));
        assert!(truncated.contains("..."));
        // Tiny limits degrade gracefully without panicking.
        assert_eq!(truncate_status("abcdef", 3).chars().count(), 3);
    }
}
