//! Bounded UTF-8 sidecar subtitles. Plain text only; never HTML or executable CSS.

#[cfg(test)]
mod tests;

/// Maximum encoded bytes in one track (hosts must also cap aggregate downloads).
pub const MAX_SUBTITLE_BYTES: usize = 1024 * 1024;
/// Maximum number of cues in one track.
pub const MAX_SUBTITLE_CUES: usize = 10_000;
/// Maximum retained UTF-8 bytes in a cue.
pub const MAX_CUE_BYTES: usize = 4096;
/// Maximum simultaneous cues, enforced before a track can be used.
pub const MAX_ACTIVE_CUES: usize = 8;
/// Maximum rendered UTF-8 text per observation, including line separators.
pub const MAX_DISPLAY_BYTES: usize = 8192;
const MAX_TIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Accepted plain-text sidecar encodings. Layout/styling are not rendered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleFormat {
    /// SubRip timestamps with comma milliseconds and optional numeric cue IDs.
    SubRip,
    /// WebVTT timestamps, cue IDs, NOTE blocks and inline text annotations.
    WebVtt,
}

/// Redacted parse failures; never includes subtitle text or a source URL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleError {
    /// Invalid encoding, timestamps, controls or malformed cue structure.
    Invalid,
    /// Input, time, cue, overlap or display limit exceeded.
    Limit,
    /// CSS, regions, cue layout or timestamp-map features are not implemented.
    Unsupported,
}

struct Cue {
    start: u64,
    end: u64,
    text: String,
}

/// Immutable millisecond media timeline; lookup does not advance its own clock.
/// Deliberately has no Debug implementation that could expose private captions.
pub struct SubtitleTrack {
    cues: Vec<Cue>,
}

impl SubtitleTrack {
    /// Parses a complete bounded track. No partial track is returned on error.
    pub fn parse(bytes: &[u8], format: SubtitleFormat) -> Result<Self, SubtitleError> {
        if bytes.len() > MAX_SUBTITLE_BYTES {
            return Err(SubtitleError::Limit);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| SubtitleError::Invalid)?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(SubtitleError::Invalid);
        }
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines = normalized.lines().peekable();
        if format == SubtitleFormat::WebVtt {
            let first = lines.next().ok_or(SubtitleError::Invalid)?;
            if !(first == "WEBVTT" || first.starts_with("WEBVTT ") || first.starts_with("WEBVTT\t"))
                || first.contains("-->")
            {
                return Err(SubtitleError::Invalid);
            }
            // Header metadata such as X-TIMESTAMP-MAP needs a different clock contract.
            if lines.next().is_some_and(|line| !line.trim().is_empty()) {
                return Err(SubtitleError::Unsupported);
            }
        }
        let mut cues: Vec<Cue> = Vec::new();
        while let Some(mut line) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }
            if format == SubtitleFormat::WebVtt
                && (line == "NOTE" || line.starts_with("NOTE ") || line.starts_with("NOTE\t"))
            {
                for note in lines.by_ref() {
                    if note.trim().is_empty() {
                        break;
                    }
                }
                continue;
            }
            if matches!(line, "STYLE" | "REGION") {
                return Err(SubtitleError::Unsupported);
            }
            if !line.contains("-->") {
                if format == SubtitleFormat::SubRip && !line.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(SubtitleError::Invalid);
                }
                line = lines.next().ok_or(SubtitleError::Invalid)?;
            }
            let (start, end) = line.split_once("-->").ok_or(SubtitleError::Invalid)?;
            let parts: Vec<_> = end.split_ascii_whitespace().collect();
            if parts.len() != 1 {
                return Err(SubtitleError::Unsupported);
            }
            let start = timestamp(start.trim(), format)?;
            let end = timestamp(parts[0], format)?;
            if end <= start || cues.last().is_some_and(|cue| cue.start > start) {
                return Err(SubtitleError::Invalid);
            }
            let mut payload = String::new();
            for line in lines.by_ref() {
                if line.trim().is_empty() {
                    break;
                }
                if payload.len() + line.len() + 1 > MAX_CUE_BYTES {
                    return Err(SubtitleError::Limit);
                }
                if !payload.is_empty() {
                    payload.push('\n');
                }
                payload.push_str(line);
            }
            let text = plain_text(&payload)?;
            if text.is_empty() {
                return Err(SubtitleError::Invalid);
            }
            if cues.len() == MAX_SUBTITLE_CUES {
                return Err(SubtitleError::Limit);
            }
            cues.push(Cue { start, end, text });
        }
        if cues.is_empty() {
            return Err(SubtitleError::Invalid);
        }
        // End events precede starts at identical times: intervals are [start, end).
        let mut events = Vec::with_capacity(cues.len() * 2);
        for cue in &cues {
            events.push((cue.start, 1i32, cue.text.len() + 1));
            events.push((cue.end, -1i32, cue.text.len() + 1));
        }
        events.sort_unstable();
        let (mut active, mut size) = (0i32, 0usize);
        for (_, change, bytes) in events {
            active += change;
            if change > 0 {
                size += bytes;
            } else {
                size -= bytes;
            }
            if active > MAX_ACTIVE_CUES as i32 || size > MAX_DISPLAY_BYTES {
                return Err(SubtitleError::Limit);
            }
        }
        Ok(Self { cues })
    }

    /// Count for payload-free diagnostics.
    pub fn cue_count(&self) -> usize {
        self.cues.len()
    }

    /// Returns visible plain text at the OS player's current media position.
    /// Arbitrary backward seeks are supported; pause simply repeats the position.
    pub fn text_at(&self, position_ms: u64) -> String {
        let mut text = String::new();
        let stop = self.cues.partition_point(|cue| cue.start <= position_ms);
        for cue in &self.cues[..stop] {
            if position_ms < cue.end {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&cue.text);
            }
        }
        text
    }
}

fn timestamp(value: &str, format: SubtitleFormat) -> Result<u64, SubtitleError> {
    let separator = if format == SubtitleFormat::SubRip {
        ','
    } else {
        '.'
    };
    let (clock, ms) = value.split_once(separator).ok_or(SubtitleError::Invalid)?;
    let parts: Vec<_> = clock.split(':').collect();
    if !(parts.len() == 3 || (format == SubtitleFormat::WebVtt && parts.len() == 2))
        || ms.len() != 3
    {
        return Err(SubtitleError::Invalid);
    }
    let number = |s: &str| -> Result<u64, SubtitleError> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(SubtitleError::Invalid);
        }
        s.parse().map_err(|_| SubtitleError::Limit)
    };
    let seconds = parts[parts.len() - 1];
    let minutes = parts[parts.len() - 2];
    if seconds.len() != 2 || minutes.len() != 2 || number(seconds)? >= 60 || number(minutes)? >= 60
    {
        return Err(SubtitleError::Invalid);
    }
    let hours = if parts.len() == 3 {
        if parts[0].len() < 2 {
            return Err(SubtitleError::Invalid);
        }
        number(parts[0])?
    } else {
        0
    };
    hours
        .checked_mul(3_600_000)
        .and_then(|n| {
            n.checked_add(
                number(minutes).ok()? * 60_000 + number(seconds).ok()? * 1000 + number(ms).ok()?,
            )
        })
        .filter(|n| *n <= MAX_TIME_MS)
        .ok_or(SubtitleError::Limit)
}

fn plain_text(value: &str) -> Result<String, SubtitleError> {
    let mut output = String::new();
    let mut rest = value;
    while !rest.is_empty() {
        if let Some(tag) = rest.strip_prefix('<') {
            let end = tag.find('>').ok_or(SubtitleError::Invalid)?;
            if end > 256 {
                return Err(SubtitleError::Limit);
            }
            let name = tag[..end]
                .trim_start_matches('/')
                .split([' ', '.', '\t'])
                .next()
                .unwrap_or_default();
            if matches!(
                name,
                "b" | "i" | "u" | "c" | "v" | "lang" | "ruby" | "rt" | "font"
            ) || timestamp(&tag[..end], SubtitleFormat::WebVtt).is_ok()
            {
                rest = &tag[end + 1..];
                continue;
            }
            // Unknown markup remains visible literal text, never interpreted.
        }
        let mut entity = false;
        for (encoded, decoded) in [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&nbsp;", "\u{a0}"),
            ("&lrm;", "\u{200e}"),
            ("&rlm;", "\u{200f}"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ] {
            if let Some(tail) = rest.strip_prefix(encoded) {
                output.push_str(decoded);
                rest = tail;
                entity = true;
                break;
            }
        }
        if !entity {
            let c = rest.chars().next().ok_or(SubtitleError::Invalid)?;
            output.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(output)
}
