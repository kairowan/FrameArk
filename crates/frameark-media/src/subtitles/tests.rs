use super::*;

#[test]
fn srt_unicode_overlap_pause_and_arbitrary_seek() {
    let track = SubtitleTrack::parse("\u{feff}1\r\n00:00:00,500 --> 00:00:02,000\r\n<b>帧舟</b> &amp; hello\r\nsecond line\r\n\r\n2\r\n00:00:01,000 --> 00:00:03,000\r\nOverlap\r\n".as_bytes(), SubtitleFormat::SubRip).unwrap();
    assert_eq!(track.cue_count(), 2);
    assert_eq!(track.text_at(499), "");
    assert_eq!(track.text_at(500), "帧舟 & hello\nsecond line");
    assert_eq!(track.text_at(1000), "帧舟 & hello\nsecond line\nOverlap");
    assert_eq!(track.text_at(2000), "Overlap");
    assert_eq!(track.text_at(2000), "Overlap");
    assert_eq!(track.text_at(3000), "");
    assert_eq!(track.text_at(600), "帧舟 & hello\nsecond line");
    assert_eq!(track.text_at(u64::MAX), "");
}

#[test]
fn vtt_notes_ids_annotations_and_plain_text() {
    let track = SubtitleTrack::parse(b"WEBVTT original fixture\n\nNOTE ignored\nprivate note\n\ncue-a\n00:01.000 --> 00:02.500\n<v Speaker><c.green>hello</c></v> &lt;b&gt;literal&lt;/b&gt;\n\n00:03.000 --> 00:04.000\n<script>never executes</script>\n", SubtitleFormat::WebVtt).unwrap();
    assert_eq!(track.text_at(1000), "hello <b>literal</b>");
    assert_eq!(track.text_at(3000), "<script>never executes</script>");
    assert_eq!(track.text_at(2500), "");
}

#[test]
fn malformed_unsupported_and_resource_bounds_fail_atomically() {
    for text in [
        "",
        "WEBVTT",
        "1\n00:00:02,000 --> 00:00:01,000\nx",
        "1\n00:60:00,000 --> 00:61:00,000\nx",
        "1\n00:00:00,00 --> 00:00:01,000\nx",
        "1\n00:00:00,000 --> 00:00:01,000\n\0",
        "1\n00:00:00,000 --> 00:00:01,000\n<unclosed",
    ] {
        assert!(SubtitleTrack::parse(text.as_bytes(), SubtitleFormat::SubRip).is_err());
    }
    for text in [
        "WEBVTT\nX-TIMESTAMP-MAP=anything\n\n",
        "WEBVTT\n\nSTYLE\nbody{}",
        "WEBVTT\n\nREGION\nid:1",
        "WEBVTT\n\n00:00.000 --> 00:01.000 line:0\nx",
    ] {
        assert!(matches!(
            SubtitleTrack::parse(text.as_bytes(), SubtitleFormat::WebVtt),
            Err(SubtitleError::Unsupported)
        ));
    }
    assert!(SubtitleTrack::parse(&[255], SubtitleFormat::SubRip).is_err());
    assert!(
        SubtitleTrack::parse(&vec![b'x'; MAX_SUBTITLE_BYTES + 1], SubtitleFormat::SubRip).is_err()
    );
    let oversized = format!(
        "00:00:00,000 --> 00:00:01,000\n{}",
        "x".repeat(MAX_CUE_BYTES + 1)
    );
    assert!(SubtitleTrack::parse(oversized.as_bytes(), SubtitleFormat::SubRip).is_err());
    assert!(timestamp("18446744073709551615:00:00,000", SubtitleFormat::SubRip).is_err());
    assert!(timestamp("168:00:00,001", SubtitleFormat::SubRip).is_err());
    let clock = |seconds: usize| {
        format!(
            "{:02}:{:02}:{:02},000",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60
        )
    };
    let sequential = (0..MAX_SUBTITLE_CUES)
        .map(|i| format!("{} --> {}\nx\n\n", clock(i), clock(i + 1)))
        .collect::<String>();
    assert_eq!(
        SubtitleTrack::parse(sequential.as_bytes(), SubtitleFormat::SubRip)
            .unwrap()
            .cue_count(),
        MAX_SUBTITLE_CUES
    );
    assert!(matches!(
        SubtitleTrack::parse(
            format!("{sequential}03:00:00,000 --> 03:00:01,000\nx").as_bytes(),
            SubtitleFormat::SubRip,
        ),
        Err(SubtitleError::Limit)
    ));
}

#[test]
fn overlap_and_total_render_size_are_validated_before_use() {
    let cue = "00:00:00,000 --> 00:00:01,000\nx\n\n";
    assert!(SubtitleTrack::parse(cue.repeat(8).as_bytes(), SubtitleFormat::SubRip).is_ok());
    assert!(matches!(
        SubtitleTrack::parse(cue.repeat(9).as_bytes(), SubtitleFormat::SubRip),
        Err(SubtitleError::Limit)
    ));
    let large = format!("00:00:00,000 --> 00:00:01,000\n{}\n\n", "x".repeat(3000));
    assert!(matches!(
        SubtitleTrack::parse(large.repeat(3).as_bytes(), SubtitleFormat::SubRip),
        Err(SubtitleError::Limit)
    ));
    let adjacent = b"00:00:00,000 --> 00:00:01,000\nx\n\n00:00:01,000 --> 00:00:02,000\ny";
    assert_eq!(
        SubtitleTrack::parse(adjacent, SubtitleFormat::SubRip)
            .unwrap()
            .text_at(1000),
        "y"
    );
}

#[test]
fn arbitrary_short_bytes_never_panic_or_exceed_display_bounds() {
    let mut state = 0x1234abcd_u32;
    for length in 0..512 {
        let bytes: Vec<_> = (0..length)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect();
        for format in [SubtitleFormat::SubRip, SubtitleFormat::WebVtt] {
            if let Ok(track) = SubtitleTrack::parse(&bytes, format) {
                assert!(track.text_at(1000).len() <= MAX_DISPLAY_BYTES);
            }
        }
    }
}
