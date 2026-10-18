// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Taking a stretch of time out of a lane, and closing the lane behind it.
//!
//! This is the primitive *Remove silence* is built on: silence detection
//! returns ranges, and each one becomes one of these. It is also what a
//! timeline needs for "delete this time selection" once the editor has one.
//!
//! It exists as a command rather than a batch of the commands already here
//! because the operation cannot be composed from them in a single undo step.
//! Cutting a span out of the middle of a clip splits it, [`Command::SplitClips`]
//! mints the tail piece's id at apply time, and a [`Command::Batch`] is built
//! before any of it runs - so nothing in it can name the clip it means to
//! delete. One command, one id minted inside it, one undo step.
//!
//! What each affected clip becomes:
//!
//! | Where the span falls | What is left |
//! |---|---|
//! | over the whole clip | nothing |
//! | over its head | the tail, its in-point moved past the span, pulled back to where the span began |
//! | over its tail | the head |
//! | across its middle | two clips: the head, and the tail pulled back to where the span began |
//!
//! Keyframes ride with their clip (`rewindow_keys`), so a fade measured in
//! clip time still fades where it did. A piece left shorter than the minimum
//! duration is dropped rather than kept as a sliver.

use super::*;

/// How close two edges have to be before they are the same edge, as upstream's
/// clip commands have it. A clip that ends exactly where the span starts is
/// not touched by the span.
const JOIN_EPSILON: f64 = 1e-6;

/// Applies this module's one command. Any other is a routing error.
pub(super) fn apply(
    project: &mut Project,
    mint: &mut IdMint,
    command: Command,
) -> Result<Outcome, CommandError> {
    let Command::RemoveSpan {
        track_id,
        start,
        end,
        ripple,
    } = command
    else {
        unreachable!("commands::apply routes only this module's command here")
    };
    let start = start.max(0.0);
    if !end.is_finite() || !start.is_finite() || end <= start {
        return Ok(Outcome::default());
    }
    let removed = end - start;
    let timeline = project.active_mut();
    if timeline.track(&track_id).is_none() {
        return Err(CommandError::TrackGone);
    }

    let mut gone: Vec<String> = Vec::new();
    let mut tails: Vec<Clip> = Vec::new();
    let mut applied = false;

    for clip in timeline.clips_where(|clip| {
        clip.track_id == track_id
            && clip.start < end - JOIN_EPSILON
            && clip.start + clip.duration > start + JOIN_EPSILON
    }) {
        let clip_start = clip.start;
        let clip_end = clip.start + clip.duration;
        let old = clip.duration;

        // The span is the whole clip.
        if clip_start >= start - JOIN_EPSILON && clip_end <= end + JOIN_EPSILON {
            gone.push(clip.id.clone());
            applied = true;
            continue;
        }

        // The span is over the head: the tail keeps playing from past it, and
        // the ripple will pull it back to where the span began.
        if clip_start >= start - JOIN_EPSILON {
            let lost = end - clip_start;
            let duration = clip_end - end;
            clip.source_start += lost * clip.speed;
            clip.start = start;
            clip.duration = duration;
            clip.rewindow_keys(old, lost, old);
            if duration < MIN_CLIP_DURATION {
                gone.push(clip.id.clone());
            }
            applied = true;
            continue;
        }

        // The span is over the tail, or across the middle. Either way the head
        // ends where the span begins.
        let head = start - clip_start;
        if head < MIN_CLIP_DURATION {
            gone.push(clip.id.clone());
        } else {
            clip.duration = head;
            clip.rewindow_keys(old, 0.0, head);
        }
        applied = true;

        // Across the middle there is a tail to keep: a clip of its own, at the
        // span's start, reading from past the span.
        if clip_end > end + JOIN_EPSILON {
            let duration = clip_end - end;
            if duration >= MIN_CLIP_DURATION {
                let mut tail = clip.clone();
                tail.id = mint.next("c");
                tail.start = start;
                tail.source_start = clip.source_start + (end - clip_start) * clip.speed;
                tail.duration = duration;
                tail.rewindow_keys(old, end - clip_start, clip_end - clip_start);
                tails.push(tail);
            }
        }
    }

    if !applied && tails.is_empty() {
        return Ok(Outcome::default());
    }
    timeline.clips.retain(|clip| !gone.contains(&clip.id));
    timeline.clips.extend(tails.into_iter().map(Arc::new));

    // The lane behind the cut closes. The pieces that were just kept sit at
    // the span's start, before the gate, so they are not moved twice.
    if ripple {
        for other in timeline
            .clips_where(|clip| clip.track_id == track_id && clip.start >= end - JOIN_EPSILON)
        {
            other.start = (other.start - removed).max(0.0);
        }
    }

    Ok(Outcome {
        created_id: None,
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use crate::commands::NewMedia;

    /// An editor holding one ten-second clip of a ten-second file at the head
    /// of the first lane, and the lane's id.
    fn fixture() -> (Editor, String) {
        let mut editor = Editor::new();
        let media_id = editor
            .apply(Command::AddMedia {
                item: NewMedia {
                    path: "/a.mp4".to_owned(),
                    name: "a.mp4".to_owned(),
                    duration: Some(10.0),
                    kind: MediaKind::Video,
                    width: Some(1920),
                    height: Some(1080),
                    frame_rate: Some(30.0),
                    frame_rate_fraction: Some("30/1".to_owned()),
                    video_codec: Some("h264".to_owned()),
                    audio_codec: Some("aac".to_owned()),
                    has_audio: true,
                    audio_tracks: Vec::new(),
                    origin: None,
                    color_space: Default::default(),
                },
            })
            .expect("adds the media")
            .created_id
            .expect("an id");
        let track_id = editor.project().active().tracks[0].id.clone();
        for start in [0.0, 10.0] {
            editor
                .apply(Command::AddClip {
                    media_id: media_id.clone(),
                    track_id: track_id.clone(),
                    start,
                    ripple: false,
                })
                .expect("adds the clip");
        }
        (editor, track_id)
    }

    fn spans(editor: &Editor) -> Vec<(f64, f64, f64)> {
        let mut out: Vec<(f64, f64, f64)> = editor
            .project()
            .active()
            .clips
            .iter()
            .map(|clip| (clip.start, clip.duration, clip.source_start))
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    fn cut(editor: &mut Editor, track_id: &str, start: f64, end: f64) {
        editor
            .apply(Command::RemoveSpan {
                track_id: track_id.to_owned(),
                start,
                end,
                ripple: true,
            })
            .expect("cuts");
    }

    #[test]
    fn a_span_across_the_middle_of_a_clip_leaves_two_and_pulls_the_tail_back() {
        let (mut editor, track) = fixture();
        cut(&mut editor, &track, 4.0, 6.0);
        let found = spans(&editor);
        assert_eq!(found.len(), 3, "{found:?}");
        // The head keeps playing from where it was.
        assert_eq!((found[0].0, found[0].1, found[0].2), (0.0, 4.0, 0.0));
        // The tail starts where the span did, reading from past it, four
        // seconds of the file that are left, and the lane behind it closed by
        // the two seconds the span took.
        assert_eq!((found[1].0, found[1].1, found[1].2), (4.0, 4.0, 6.0));
        assert_eq!((found[2].0, found[2].1, found[2].2), (8.0, 10.0, 0.0));
    }

    #[test]
    fn a_span_over_the_tail_of_a_clip_shortens_it_and_closes_the_lane() {
        let (mut editor, track) = fixture();
        cut(&mut editor, &track, 8.0, 10.0);
        let found = spans(&editor);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (0.0, 8.0, 0.0));
        // The clip that began exactly where the span ended is not touched by
        // it, and the lane closed behind the cut by its two seconds.
        assert_eq!((found[1].0, found[1].1, found[1].2), (8.0, 10.0, 0.0));
    }

    #[test]
    fn a_span_over_the_head_of_a_clip_keeps_the_rest_and_moves_its_in_point() {
        let (mut editor, track) = fixture();
        cut(&mut editor, &track, 10.0, 13.0);
        let found = spans(&editor);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (0.0, 10.0, 0.0));
        assert_eq!(
            (found[1].0, found[1].1, found[1].2),
            (10.0, 7.0, 3.0),
            "seven seconds of the file, from its third"
        );
    }

    #[test]
    fn a_span_over_a_whole_clip_takes_it_and_leaves_no_hole() {
        let (mut editor, track) = fixture();
        cut(&mut editor, &track, 0.0, 10.0);
        let found = spans(&editor);
        assert_eq!(found, vec![(0.0, 10.0, 0.0)], "{found:?}");
    }

    #[test]
    fn a_span_that_misses_every_clip_changes_nothing() {
        let (mut editor, track) = fixture();
        let before = spans(&editor);
        let outcome = editor
            .apply(Command::RemoveSpan {
                track_id: track,
                start: 30.0,
                end: 31.0,
                ripple: true,
            })
            .expect("cuts");
        assert!(!outcome.applied);
        assert_eq!(spans(&editor), before);
    }
}
