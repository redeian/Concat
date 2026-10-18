// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Finding the dead air in a stretch of sound.
//!
//! A talking head, a screen recording, a podcast: most of the timeline is not
//! words. Cutting it by hand is the slowest part of a first pass, and it is
//! the part every editor automates, so this is the analysis behind
//! *Remove silence*.
//!
//! The measurement is the one every tool in this space makes: windows of a few
//! milliseconds, each taken to its RMS level in dBFS, a window below the
//! threshold counted as silent, runs of silent windows longer than the minimum
//! reported as silence to cut. Where a window's channels disagree the loudest
//! one decides - FFmpeg's `silencedetect` treats a frame as silence only when
//! *every* channel is silent, and a stereo recording with a noisy left channel
//! is not silent.
//!
//! [`Settings::default`] follows the field, not this crate's taste:
//!
//! | Tool | Threshold | Minimum | Padding |
//! |---|---|---|---|
//! | FFmpeg `silencedetect` defaults | -60 dB | 2 s | none |
//! | FFmpeg as usually driven | -30 dB | 0.5 s | none |
//! | FireCut (Premiere) recommends | -30 dBFS (-35 advised) | 750 ms | 250 ms each side |
//! | VideoCascade API | -30 dB | 0.5 s | none |
//! | auto-editor default edit | ~-28 dB (4% amplitude) | - | 0.2 s margin |
//!
//! So: -30 dBFS, half a second, and a fifth of a second of padding either side
//! - the padding is what keeps a plosive or a trailing breath from being
//! clipped off the front of the next word, and it is why the default cut
//! sounds like an edit rather than like a machine.
//!
//! What this does *not* do is tell speech from music or from a noise floor
//! that sits above the threshold. A room with air conditioning at -28 dBFS
//! reads as speech at -30. [`suggest_threshold_db`] measures the material and
//! proposes a threshold between its two levels, which is what the better tools
//! offer; deciding there is nothing to cut is still the editor's call.

use std::ops::Range;

/// How long a window is, in seconds. Ten milliseconds is 100 levels a second:
/// fine enough that a cut lands within a frame of the word it follows, coarse
/// enough that an hour of sound is 360,000 numbers rather than 173 million.
const WINDOW: f64 = 0.010;

/// The floor a level is clamped to, so silence reads as a number rather than
/// as negative infinity.
const FLOOR_DB: f64 = -100.0;

/// What counts as silence, and what may be taken out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// A window quieter than this is silent. dBFS: 0 is full scale, so this is
    /// always negative.
    pub threshold_db: f64,
    /// A run of silence shorter than this is a pause, not a cut. Seconds.
    pub min_silence: f64,
    /// Taken off each end of a silence found, so the cut never lands on the
    /// attack of a word or the tail of one. Seconds.
    pub padding: f64,
    /// The shortest stretch of sound left between two cuts. Two silences
    /// closer together than this become one, because a word held for a fifth
    /// of a second between two cuts sounds like a stutter. Seconds.
    pub min_gap: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            threshold_db: -30.0,
            min_silence: 0.5,
            padding: 0.2,
            min_gap: 0.3,
        }
    }
}

impl Settings {
    /// The same settings, held to what the analysis can honour: a threshold on
    /// the dBFS scale, no negative durations, and a window no longer than the
    /// shortest silence - a minimum shorter than one window could never be met.
    fn sane(self) -> Settings {
        Settings {
            threshold_db: self.threshold_db.clamp(FLOOR_DB, 0.0),
            min_silence: self.min_silence.max(WINDOW),
            padding: self.padding.max(0.0),
            min_gap: self.min_gap.max(0.0),
        }
    }
}

/// The level of each window of `samples`, in dBFS. `samples` is interleaved;
/// `channels` is how many samples make a frame.
fn levels(samples: &[f32], rate: u32, channels: u16) -> Vec<f64> {
    let channels = channels.max(1) as usize;
    let per_window = ((rate as f64 * WINDOW).round() as usize).max(1) * channels;
    if samples.is_empty() || per_window == 0 {
        return Vec::new();
    }
    samples
        .chunks(per_window)
        .map(|window| {
            // The loudest channel decides, as FFmpeg's silencedetect does: a
            // frame is silent only when every channel is.
            let loudest = (0..channels)
                .map(|channel| {
                    let mut sum = 0.0_f64;
                    let mut count = 0_usize;
                    for frame in window.chunks(channels) {
                        if let Some(sample) = frame.get(channel) {
                            let sample = f64::from(*sample);
                            sum += sample * sample;
                            count += 1;
                        }
                    }
                    if count == 0 {
                        0.0
                    } else {
                        (sum / count as f64).sqrt()
                    }
                })
                .fold(0.0_f64, f64::max);
            if loudest <= 0.0 {
                FLOOR_DB
            } else {
                (20.0 * loudest.log10()).max(FLOOR_DB)
            }
        })
        .collect()
}

/// The silences in `samples` worth cutting, in seconds from the start of the
/// analysed stretch, already padded and already merged.
///
/// Ranges are in order, do not overlap, and are all at least
/// [`Settings::min_silence`] long before padding - so a range narrower than
/// its padding disappears rather than becoming a zero-length cut.
pub fn detect(
    samples: &[f32],
    rate: u32,
    channels: u16,
    settings: &Settings,
) -> Vec<Range<f64>> {
    let settings = settings.sane();
    let levels = levels(samples, rate, channels);
    if levels.is_empty() {
        return Vec::new();
    }
    let seconds_of = |window: usize| window as f64 * WINDOW;

    // Runs of windows below the threshold.
    let mut runs: Vec<Range<usize>> = Vec::new();
    let mut opened: Option<usize> = None;
    for (window, level) in levels.iter().enumerate() {
        let silent = *level < settings.threshold_db;
        match (silent, opened) {
            (true, None) => opened = Some(window),
            (false, Some(start)) => {
                runs.push(start..window);
                opened = None;
            }
            _ => {}
        }
    }
    if let Some(start) = opened {
        runs.push(start..levels.len());
    }

    // Long enough to be a cut, then padded at both ends.
    let mut found: Vec<Range<f64>> = runs
        .into_iter()
        .filter(|run| seconds_of(run.len()) >= settings.min_silence)
        .filter_map(|run| {
            let start = seconds_of(run.start) + settings.padding;
            let end = seconds_of(run.end) - settings.padding;
            (end > start).then_some(start..end)
        })
        .collect();

    // Two cuts with a word between them become one cut: what is left of that
    // word is too short to read as speech.
    let mut merged: Vec<Range<f64>> = Vec::with_capacity(found.len());
    for range in found.drain(..) {
        match merged.last_mut() {
            Some(last) if range.start - last.end < settings.min_gap => last.end = range.end,
            _ => merged.push(range),
        }
    }
    merged
}

/// The threshold this material asks for, in dBFS: between the level its sound
/// sits at and the level its silence sits at.
///
/// The histogram of window levels usually has two peaks - the speaker, and the
/// room - and the threshold belongs in the valley between them, which is what
/// the tools that offer an automatic threshold do. When there is only one peak
/// (a file that is nothing but speech, or nothing but room tone) there is no
/// valley to find, and the answer falls back to "well below the sound" rather
/// than guessing at a second peak that is not there.
///
/// Always inside `-60..=-18`, which is the range that does more good than
/// harm: below the bottom nothing but digital silence is cut, above the top
/// ordinary speech starts to be.
pub fn suggest_threshold_db(samples: &[f32], rate: u32, channels: u16) -> f64 {
    let levels = levels(samples, rate, channels);
    if levels.is_empty() {
        return Settings::default().threshold_db;
    }
    // One dB bins from the floor to full scale.
    const BOTTOM: f64 = -70.0;
    const TOP: f64 = 0.0;
    let bins = ((TOP - BOTTOM) as usize) + 1;
    let mut histogram = vec![0_usize; bins];
    for level in &levels {
        let bin = (level - BOTTOM).round();
        if (0.0..bins as f64).contains(&bin) {
            let bin = bin as usize;
            histogram[bin] += 1;
        }
    }
    let at = |bin: usize| BOTTOM + bin as f64;

    // The sound: the fullest bin above the bottom few decibels, since a file
    // with any digital silence at all would otherwise pick that.
    let loud_bin = (histogram.len() / 7..bins)
        .max_by_key(|bin| histogram[*bin])
        .unwrap_or(bins - 1);
    let loud = at(loud_bin);

    // The room: the fullest bin at least 12 dB under the sound.
    let quiet_ceiling = loud_bin.saturating_sub(12);
    let quiet = (0..quiet_ceiling.max(1))
        .max_by_key(|bin| histogram[*bin])
        .filter(|bin| histogram[*bin] > 0)
        .map(at);

    match quiet {
        Some(quiet) => ((loud + quiet) / 2.0).clamp(-60.0, -18.0),
        None => (loud - 25.0).clamp(-60.0, -18.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A second of a sine at `amplitude`, mono.
    fn tone(amplitude: f32) -> Vec<f32> {
        (0..RATE)
            .map(|index| {
                let phase = index as f32 / RATE as f32 * 440.0 * std::f32::consts::TAU;
                (phase.sin()) * amplitude
            })
            .collect()
    }

    /// A second of nothing, mono.
    fn quiet() -> Vec<f32> {
        vec![0.0; RATE as usize]
    }

    fn joined(parts: &[Vec<f32>]) -> Vec<f32> {
        parts.iter().flat_map(|part| part.clone()).collect()
    }

    fn settings() -> Settings {
        Settings {
            threshold_db: -30.0,
            min_silence: 0.5,
            padding: 0.0,
            min_gap: 0.0,
        }
    }

    #[test]
    fn a_second_of_silence_either_side_of_a_word_is_found() {
        let samples = joined(&[quiet(), tone(0.5), quiet()]);
        let found = detect(&samples, RATE, 1, &settings());
        assert_eq!(found.len(), 2, "{found:?}");
        assert!((found[0].start - 0.0).abs() < 0.02, "{found:?}");
        assert!((found[0].end - 1.0).abs() < 0.02, "{found:?}");
        assert!((found[1].start - 2.0).abs() < 0.02, "{found:?}");
        assert!((found[1].end - 3.0).abs() < 0.02, "{found:?}");
    }

    #[test]
    fn padding_is_taken_off_both_ends_of_what_is_cut() {
        let samples = joined(&[quiet(), tone(0.5), quiet()]);
        let mut padded = settings();
        padded.padding = 0.2;
        let found = detect(&samples, RATE, 1, &padded);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!((found[0].start - 0.2).abs() < 0.02, "{found:?}");
        assert!((found[0].end - 0.8).abs() < 0.02, "{found:?}");
        assert!((found[1].start - 2.2).abs() < 0.02, "{found:?}");
        assert!((found[1].end - 2.8).abs() < 0.02, "{found:?}");
    }

    #[test]
    fn a_pause_shorter_than_the_minimum_is_left_alone() {
        // A fifth of a second of silence: a breath, not a cut.
        let breath = vec![0.0; (RATE as f64 * 0.2) as usize];
        let samples = joined(&[tone(0.5), breath, tone(0.5)]);
        let found = detect(&samples, RATE, 1, &settings());
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn two_silences_a_word_apart_become_one_cut() {
        // A second out, a tenth of a second of word, a second out again: what
        // is left between the two cuts is too short to read as speech.
        let mut samples = quiet();
        let word = tone(0.5);
        samples.extend(word.into_iter().take((RATE as f64 * 0.1) as usize));
        samples.extend(quiet());
        let mut merging = settings();
        merging.min_silence = 0.4;
        merging.min_gap = 0.3;
        let found = detect(&samples, RATE, 1, &merging);
        assert_eq!(
            found.len(),
            1,
            "a tenth of a second of sound is not a gap: {found:?}"
        );
        assert!((found[0].start - 0.0).abs() < 0.02, "{found:?}");
        assert!((found[0].end - 2.1).abs() < 0.02, "{found:?}");
    }

    #[test]
    fn a_padded_silence_that_does_not_survive_its_padding_is_dropped() {
        // Exactly the minimum long, padded by more than half its length.
        let half = vec![0.0_f32; (RATE as f64 * 0.5) as usize];
        let samples = joined(&[tone(0.5), half, tone(0.5)]);
        let mut greedy = settings();
        greedy.min_silence = 0.5;
        greedy.padding = 0.3;
        let found = detect(&samples, RATE, 1, &greedy);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_stereo_track_is_silent_only_when_both_channels_are() {
        // Left is the room, right is the speaker: not silence.
        let mut samples = Vec::new();
        for index in 0..(RATE as usize) {
            let phase = index as f32 / RATE as f32 * 440.0 * std::f32::consts::TAU;
            samples.push(0.0);
            samples.push(phase.sin() * 0.5);
        }
        let found = detect(&samples, RATE, 2, &settings());
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_suggested_threshold_lands_between_the_room_and_the_speaker() {
        // Room at -60 dBFS, speech at about -20 dBFS.
        let room = tone(0.001);
        let speech = tone(0.1);
        let samples = joined(&[room.clone(), speech.clone(), room, speech]);
        let threshold = suggest_threshold_db(&samples, RATE, 1);
        assert!(
            (-60.0..=-18.0).contains(&threshold),
            "outside the range that does more good than harm: {threshold}"
        );
        assert!(
            threshold > -60.0 && threshold < -20.0,
            "should sit between the two levels: {threshold}"
        );
    }

    #[test]
    fn a_threshold_above_full_scale_is_held_to_it() {
        let settings = Settings {
            threshold_db: 12.0,
            ..Settings::default()
        }
        .sane();
        assert_eq!(settings.threshold_db, 0.0);
        let settings = Settings {
            min_silence: 0.0,
            ..Settings::default()
        }
        .sane();
        assert_eq!(settings.min_silence, WINDOW);
    }

    #[test]
    fn nothing_at_all_is_no_silence_to_cut() {
        assert!(detect(&[], RATE, 1, &settings()).is_empty());
        assert!(detect(&[], RATE, 1, &Settings::default()).is_empty());
    }
}
