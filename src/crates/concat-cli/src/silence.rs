// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! `concat-cli silence`: what the *Remove silence* feature would cut.
//!
//! The window's version of this runs inside a project, over a lane's clips.
//! This one runs over a file, which is what makes the analysis testable
//! against real footage before any of the cutting exists: point it at a
//! recording and it prints the ranges it would take out, with the levels it
//! measured, so the thresholds can be judged instead of guessed at.
//!
//! Decoding is 8 kHz mono whatever the file is. Silence is a question about
//! loudness over tens of milliseconds, so the rate buys nothing above a
//! telephone's - and an hour at 48 kHz stereo is 1.4 GB of floats where the
//! same hour at 8 kHz mono is 115 MB.

use std::path::Path;

use concat_media::silence::{self, Settings};
use concat_media::{AudioDecoder, AudioOptions, SampleFormat};

/// The rate the analysis runs at, and why it is not the file's own.
const ANALYSIS_RATE: u32 = 8_000;

/// Prints what `path` would lose. `threshold` is the caller's, or - when it is
/// `None` - the one the material itself suggests.
pub fn run(
    path: &Path,
    threshold: Option<f64>,
    min_silence: f64,
    padding: f64,
    min_gap: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = AudioDecoder::open(
        path,
        &AudioOptions {
            start: None,
            duration: None,
            filters: Vec::new(),
            rate: ANALYSIS_RATE,
            channels: 1,
            format: SampleFormat::F32,
            stream: None,
        },
    )?;
    let samples = decoder.collect_f32()?;
    if samples.is_empty() {
        println!("{}: no audio to analyse", path.display());
        return Ok(());
    }
    let total = samples.len() as f64 / f64::from(ANALYSIS_RATE);

    let measured = silence::suggest_threshold_db(&samples, ANALYSIS_RATE, 1);
    let settings = Settings {
        threshold_db: threshold.unwrap_or(measured),
        min_silence,
        padding,
        min_gap,
    };
    let found = silence::detect(&samples, ANALYSIS_RATE, 1, &settings);
    let took: f64 = found.iter().map(|range| range.end - range.start).sum();

    println!("{}", path.display());
    println!(
        "  {total:.2} s of audio at {ANALYSIS_RATE} Hz mono (the file's own rate is not used)"
    );
    println!(
        "  threshold {:.1} dBFS ({})",
        settings.threshold_db,
        if threshold.is_some() {
            "given"
        } else {
            "measured from this file"
        }
    );
    println!(
        "  minimum {:.2} s, padding {:.2} s each side, {:.2} s of sound kept between cuts",
        settings.min_silence, settings.padding, settings.min_gap
    );
    if found.is_empty() {
        println!("  nothing to cut");
        return Ok(());
    }
    println!(
        "  {} cut(s), {:.2} s removed, {:.0}% of the file, leaving {:.2} s",
        found.len(),
        took,
        took / total * 100.0,
        total - took
    );
    for (index, range) in found.iter().enumerate() {
        println!(
            "  {:>3}. {:>8.2} - {:>8.2}  ({:.2} s)",
            index + 1,
            range.start,
            range.end,
            range.end - range.start
        );
    }
    Ok(())
}
