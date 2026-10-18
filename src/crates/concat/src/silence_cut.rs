// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Turning silence into an edit.
//!
//! The analysis lives in `concat_media::silence` and knows nothing about a
//! timeline: it answers in seconds from the start of whatever it was given.
//! This is the joining piece - it decodes the part of the file a clip actually
//! plays, turns those seconds into timeline seconds, and hands back one
//! [`Command::Batch`] of span cuts, which is one undo step and one entry in the
//! history.
//!
//! The mapping is what needs care. A clip plays `duration * speed` seconds of
//! source from `source_start`, so a silence found `x` seconds into that window
//! is at `clip.start + x / speed` on the timeline. At `speed` 1 that is simply
//! `clip.start + x`; at half speed the same silence covers twice the timeline.
//!
//! A clip whose speed *rides* ([`Command::SetClipSpeedCurve`]) is refused
//! rather than approximated: the mapping above assumes the whole window plays
//! at one rate, and a curve would put every cut in the wrong place, which is
//! worse than saying no.
//!
//! Cuts are emitted from the last silence backwards. Each one closes the lane
//! behind it, so a cut made later in the file would move every silence before
//! it; working backwards, every range is still where the analysis found it.

use concat_media::silence::{self, Settings};
use concat_media::{AudioDecoder, AudioOptions, SampleFormat};
use concat_project::commands::Command;
use concat_project::model::{ClipKind, Project};

/// The rate the analysis runs at, as `concat-cli silence` has it: silence is a
/// question about loudness over tens of milliseconds, and an hour at 8 kHz
/// mono is 115 MB where 48 kHz stereo is 1.4 GB.
const RATE: u32 = 8_000;

/// The one batch that cuts `clip_id`'s silences out, or an empty list when
/// there is nothing worth cutting. `Err` is what the window shows the person
/// who asked.
pub fn plan(project: &Project, clip_id: &str, settings: Settings) -> Result<Vec<Command>, String> {
    let clip = project
        .active()
        .clip(clip_id)
        .ok_or_else(|| "That clip is gone.".to_owned())?;
    if !matches!(clip.kind, ClipKind::Audio | ClipKind::Video) {
        return Err("That clip has no sound.".to_owned());
    }
    if !clip.speed.is_finite() || clip.speed <= 0.0 {
        return Err("That clip's speed is not a number this can work with.".to_owned());
    }
    if clip.speed_curve.as_ref().is_some_and(|points| !points.is_empty()) {
        return Err(
            "That clip's speed rides a curve, so a silence would not land where it is cut."
                .to_owned(),
        );
    }
    let media = project
        .media_by_id(&clip.media_id)
        .ok_or_else(|| "That clip's file is no longer in the bin.".to_owned())?;
    if !media.has_audio {
        return Err("That file has no sound.".to_owned());
    }
    let window = clip.duration * clip.speed;
    if window <= 0.0 {
        return Ok(Vec::new());
    }

    let mut decoder = AudioDecoder::open(
        &media.path,
        &AudioOptions {
            start: Some(clip.source_start),
            duration: Some(window),
            filters: Vec::new(),
            rate: RATE,
            channels: 1,
            format: SampleFormat::F32,
            stream: None,
        },
    )
    .map_err(|error| format!("That file's sound could not be opened: {error}"))?;
    let samples = decoder
        .collect_f32()
        .map_err(|error| format!("That file's sound could not be read: {error}"))?;

    let found = silence::detect(&samples, RATE, 1, &settings);
    let mut commands = Vec::with_capacity(found.len());
    for range in found.iter().rev() {
        let start = clip.start + range.start / clip.speed;
        let end = clip.start + range.end / clip.speed;
        // A cut that starts and ends inside this clip, never past either edge:
        // the padding already keeps one off a word, and a span that ran past
        // the clip would take the next clip's head with it.
        let start = start.max(clip.start);
        let end = end.min(clip.start + clip.duration);
        if end - start < concat_project::model::ranges::MIN_CLIP_DURATION {
            continue;
        }
        commands.push(Command::RemoveSpan {
            track_id: clip.track_id.clone(),
            start,
            end,
            ripple: true,
        });
    }
    if commands.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Command::Batch { commands }])
}
