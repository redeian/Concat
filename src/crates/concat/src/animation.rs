// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! One-click animation for the things that are not footage: titles, shapes and
//! the stickers built from them.
//!
//! There is no animation engine here and there does not need to be one. A clip
//! already carries keyframes on its own properties - scale, offset, rotation,
//! opacity - and [`Command::SetClipKey`] writes one. A preset is therefore a
//! short list of keys, and applying it is a [`Command::Batch`]: one undo step,
//! no new engine concept, and the result is editable afterwards with the same
//! diamonds the curve editor already shows. A hand-tuned animation and a
//! preset one are the same kind of thing.
//!
//! ## Three sides, and who owns which frames
//!
//! [`Side::Enter`] keys the head of the clip, [`Side::Exit`] the tail, and
//! [`Side::During`] loops through the middle. They have to coexist on one clip -
//! a title pops in, breathes, and pops out - so each owns a stretch of the clip
//! and rewriting one never touches the others:
//!
//! | side | owns | so that |
//! |---|---|---|
//! | Enter | the first half | it survives an exit being applied after it |
//! | During | 15% to 85% | the ends stay the entrance's and the exit's |
//! | Exit | the second half | it survives an entrance being re-applied |
//!
//! Only a preset of the same side replaces what is in its own stretch. That is
//! the whole rule, and it is what a clip with `Pop in` then `Pop out` depends
//! on: applying the exit must not clear the enter that was just applied.
//!
//! ## Why these numbers
//!
//! A pop is a scale entrance with a little too much in it, and the craft
//! numbers are specific. A good pop curve reaches half its travel in about a
//! sixth of its time, passes its target by roughly fifteen per cent, and is
//! over in under 400 ms; five per cent reads as a snap, fifteen as a cartoon -
//! which is the side to err on for a title that has to catch an eye on a phone.
//! The other half of the trick is that **opacity finishes much earlier than the
//! scale does**: the words are fully there while the figure is still settling,
//! which is what makes the gesture read as one arrival rather than a fade
//! racing a resize.
//!
//! So the clip's own length is not the animation's length. The caller gives
//! seconds - 0.35 by default, 0.15 to 1.5 on the knob - and that window is
//! turned into a fraction of the clip here. A window is capped at 45% of the
//! clip so an entrance can never reach past the middle and collide with an
//! exit, which is what the ownership table above rests on.

use concat_project::commands::Command;
use concat_project::model::{Clip, KeyEase, KeyProperty};

/// The shortest and longest an animation may run, in seconds. Below the
/// shortest it is a flicker; above the longest it stops being an entrance and
/// becomes a move of its own.
pub const MIN_SECONDS: f64 = 0.15;
pub const MAX_SECONDS: f64 = 1.5;

/// What an animation runs for when nobody has said: fast enough to feel like a
/// pop, long enough to read.
pub const DEFAULT_SECONDS: f64 = 0.35;

/// The most of a clip an entrance or an exit may cover. Past this it is a move,
/// not an entrance, and it would start competing with the other end for frames.
const MAX_SPAN: f64 = 0.45;

/// Where a during animation starts and stops, as fractions of the clip. The
/// ends are kept clear for the entrance and the exit.
const DURING_FROM: f64 = 0.15;
const DURING_TO: f64 = 0.85;

/// Which part of the clip a preset keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The head: how it arrives.
    Enter,
    /// The middle: what it does while it is there.
    During,
    /// The tail: how it leaves.
    Exit,
}

/// The animations offered, in the order the inspector shows them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    // Arrivals and departures. Each has an enter and an exit shape.
    /// Scale overshoots and settles - the one everybody means by "pop".
    ElasticPop,
    /// Opacity only: the quietest arrival and departure there is.
    Fade,
    /// From below, or away upwards.
    SlideUp,
    /// From above, or away downwards.
    SlideDown,
    /// In from the left, or out to the left.
    SlideLeft,
    /// In from the right, or out to the right.
    SlideRight,
    /// Grows into place without overshoot: the calm version of the pop.
    Zoom,
    /// A quarter turn into place, with a scale to go with it.
    Spin,
    /// Falls in and settles, like it has weight.
    Drop,
    // Things that run while the clip is on screen.
    /// The heartbeat: scale up and back, three times.
    Pulse,
    /// Drifts up and back down, like it is floating.
    Float,
    /// A short, fast side-to-side shake.
    Shake,
    /// Fades down and back up: a slow breath.
    Breathe,
}

impl Preset {
    /// Everything the panel can show, in index order.
    pub const ALL: [Preset; 13] = [
        Preset::ElasticPop,
        Preset::Fade,
        Preset::SlideUp,
        Preset::SlideDown,
        Preset::SlideLeft,
        Preset::SlideRight,
        Preset::Zoom,
        Preset::Spin,
        Preset::Drop,
        Preset::Pulse,
        Preset::Float,
        Preset::Shake,
        Preset::Breathe,
    ];

    /// The presets that have an entrance and an exit, in the order the panel
    /// lists them.
    pub const ENTER_EXIT: [Preset; 9] = [
        Preset::ElasticPop,
        Preset::Fade,
        Preset::SlideUp,
        Preset::SlideDown,
        Preset::SlideLeft,
        Preset::SlideRight,
        Preset::Zoom,
        Preset::Spin,
        Preset::Drop,
    ];

    /// The presets that run for as long as the clip is on screen.
    pub const DURING: [Preset; 4] = [
        Preset::Pulse,
        Preset::Float,
        Preset::Shake,
        Preset::Breathe,
    ];

    /// The index the inspector's buttons send. Global across both lists, so a
    /// button in either group names its preset unambiguously.
    pub fn index(self) -> i32 {
        Preset::ALL
            .iter()
            .position(|preset| *preset == self)
            .map_or(0, |index| index as i32)
    }

    /// The preset a button index names, if any.
    pub fn from_index(index: i32) -> Option<Preset> {
        Preset::ALL.get(usize::try_from(index).ok()?).copied()
    }

    /// Whether this preset is one that loops rather than one that arrives.
    pub fn runs_during(self) -> bool {
        Preset::DURING.contains(&self)
    }
}

impl Side {
    /// The side a button names: 0 enter, 1 during, 2 exit.
    pub fn from_index(index: i32) -> Option<Side> {
        match index {
            0 => Some(Side::Enter),
            1 => Some(Side::During),
            2 => Some(Side::Exit),
            _ => None,
        }
    }

    /// The index the panel's buttons send.
    pub fn index(self) -> i32 {
        match self {
            Side::Enter => 0,
            Side::During => 1,
            Side::Exit => 2,
        }
    }

    /// The stretch of the clip this side owns, as fractions. A preset replaces
    /// the keys inside it and leaves everything outside alone.
    fn owns(self, at: f64) -> bool {
        match self {
            Side::Enter => at < 0.5,
            Side::During => (DURING_FROM..=DURING_TO).contains(&at),
            Side::Exit => at >= 0.5,
        }
    }

    /// Where a preset of this side starts and ends, as fractions of the clip.
    /// Used to put the playhead somewhere worth watching after an apply.
    pub fn window(self, span: f64) -> (f64, f64) {
        match self {
            Side::Enter => (0.0, span.clamp(0.02, MAX_SPAN)),
            Side::During => (DURING_FROM, DURING_TO),
            Side::Exit => (1.0 - span.clamp(0.02, MAX_SPAN), 1.0),
        }
    }
}

/// One key: where in the clip, what value, and how it is approached.
type Key = (f64, f64, KeyEase);

/// A quarter turn, in the radians `Clip::rotation` is measured in.
const QUARTER_TURN: f64 = std::f64::consts::FRAC_PI_2;

/// The keys a preset writes, per property, inside `span` - the share of the
/// clip the animation covers, as a fraction.
fn keys(preset: Preset, side: Side, span: f64) -> Vec<(KeyProperty, Vec<Key>)> {
    let span = span.clamp(0.02, MAX_SPAN);
    let at = move |part: f64| match side {
        Side::Enter => (span * part).clamp(0.0, 1.0),
        Side::Exit => (1.0 - span + span * part).clamp(0.0, 1.0),
        Side::During => (DURING_FROM + (DURING_TO - DURING_FROM) * part).clamp(0.0, 1.0),
    };
    // Out approaches the key it lands on from the other side, so the curve does
    // not slow down into the move it is making.
    let ease = match side {
        Side::Enter => KeyEase::OUT,
        Side::During => KeyEase::IN,
        Side::Exit => KeyEase::IN,
    };
    // A fifth of the frame is enough to read as movement on a 1080p cut and
    // small enough not to look like a mistake.
    let drift = 0.22;

    match (preset, side) {
        // The overshoot is the animation: small, past the mark, under it, one
        // last correction. Opacity is done a quarter of the way in.
        (Preset::ElasticPop, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.25), 1.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (at(0.0), 0.72, ease),
                    (at(0.32), 1.15, ease),
                    (at(0.55), 0.97, ease),
                    (at(0.78), 1.04, ease),
                    (at(1.0), 1.0, ease),
                ],
            ),
        ],
        // The same shape backwards, and shorter: an exit that dawdles holds the
        // cut up.
        (Preset::ElasticPop, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.55), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (at(0.0), 1.0, ease),
                    (at(0.45), 1.0, ease),
                    (at(0.62), 1.12, ease),
                    (at(1.0), 0.70, ease),
                ],
            ),
        ],
        (Preset::Fade, Side::Enter) => vec![(
            KeyProperty::Opacity,
            vec![(at(0.0), 0.0, ease), (at(0.6), 1.0, ease)],
        )],
        (Preset::Fade, Side::Exit) => vec![(
            KeyProperty::Opacity,
            vec![(at(0.4), 1.0, ease), (at(1.0), 0.0, ease)],
        )],
        (Preset::SlideUp, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.5), 1.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![(at(0.0), drift, ease), (at(1.0), 0.0, ease)],
            ),
        ],
        (Preset::SlideUp, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.5), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![
                    (at(0.0), 0.0, ease),
                    (at(0.45), 0.0, ease),
                    (at(1.0), -drift, ease),
                ],
            ),
        ],
        (Preset::SlideDown, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.5), 1.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![(at(0.0), -drift, ease), (at(1.0), 0.0, ease)],
            ),
        ],
        (Preset::SlideDown, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.5), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![
                    (at(0.0), 0.0, ease),
                    (at(0.45), 0.0, ease),
                    (at(1.0), drift, ease),
                ],
            ),
        ],
        (Preset::SlideLeft, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.4), 1.0, ease)],
            ),
            (
                KeyProperty::OffsetX,
                vec![(at(0.0), -drift, ease), (at(1.0), 0.0, ease)],
            ),
        ],
        (Preset::SlideLeft, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.4), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::OffsetX,
                vec![
                    (at(0.0), 0.0, ease),
                    (at(0.4), 0.0, ease),
                    (at(1.0), -drift, ease),
                ],
            ),
        ],
        (Preset::SlideRight, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.4), 1.0, ease)],
            ),
            (
                KeyProperty::OffsetX,
                vec![(at(0.0), drift, ease), (at(1.0), 0.0, ease)],
            ),
        ],
        (Preset::SlideRight, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.4), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::OffsetX,
                vec![
                    (at(0.0), 0.0, ease),
                    (at(0.4), 0.0, ease),
                    (at(1.0), drift, ease),
                ],
            ),
        ],
        // No overshoot at all: a zoom that bounces is a pop, and this is the
        // one you reach for when a pop would be too much.
        (Preset::Zoom, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.35), 1.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![(at(0.0), 0.86, ease), (at(1.0), 1.0, ease)],
            ),
        ],
        (Preset::Zoom, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.6), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![(at(0.0), 1.0, ease), (at(1.0), 0.84, ease)],
            ),
        ],
        // In from a quarter turn away, small and dark, so the turn is what the
        // eye follows.
        (Preset::Spin, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.3), 1.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![(at(0.0), 0.5, ease), (at(1.0), 1.0, ease)],
            ),
            (
                KeyProperty::Rotation,
                vec![(at(0.0), -QUARTER_TURN, ease), (at(1.0), 0.0, ease)],
            ),
        ],
        (Preset::Spin, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.7), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::Scale,
                vec![(at(0.0), 1.0, ease), (at(1.0), 0.5, ease)],
            ),
            (
                KeyProperty::Rotation,
                vec![(at(0.0), 0.0, ease), (at(1.0), QUARTER_TURN, ease)],
            ),
        ],
        // Falls past the mark, comes back a little under it, and settles: the
        // overshoot of the pop, but in one direction and with weight.
        (Preset::Drop, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, ease), (at(0.3), 1.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![
                    (at(0.0), -0.45, ease),
                    (at(0.55), 0.05, ease),
                    (at(0.78), -0.02, ease),
                    (at(1.0), 0.0, ease),
                ],
            ),
        ],
        (Preset::Drop, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.6), 1.0, ease), (at(1.0), 0.0, ease)],
            ),
            (
                KeyProperty::OffsetY,
                vec![
                    (at(0.0), 0.0, ease),
                    (at(0.6), 0.0, ease),
                    (at(1.0), 0.32, ease),
                ],
            ),
        ],
        // The clip is on screen and doing something: three heartbeats.
        (Preset::Pulse, _) => {
            const CYCLES: usize = 3;
            let step = 1.0 / CYCLES as f64;
            let mut scale = Vec::new();
            for cycle in 0..CYCLES {
                let base = cycle as f64 * step;
                scale.push((at(base), 1.0, KeyEase::IN));
                scale.push((at(base + step * 0.5), 1.07, KeyEase::IN));
            }
            scale.push((at(1.0), 1.0, KeyEase::IN));
            vec![(KeyProperty::Scale, scale)]
        }
        // Small and slow: a drift, not a movement.
        (Preset::Float, _) => {
            const CYCLES: usize = 3;
            let step = 1.0 / CYCLES as f64;
            let mut offset = Vec::new();
            for cycle in 0..CYCLES {
                let base = cycle as f64 * step;
                offset.push((at(base), 0.0, KeyEase::IN));
                offset.push((at(base + step * 0.5), -0.025, KeyEase::IN));
            }
            offset.push((at(1.0), 0.0, KeyEase::IN));
            vec![(KeyProperty::OffsetY, offset)]
        }
        // Fast and tight, and small enough that it reads as energy rather than
        // as a broken position.
        (Preset::Shake, _) => {
            const CYCLES: usize = 3;
            let step = 1.0 / CYCLES as f64;
            let jitter = [0.0, 0.008, -0.008, 0.006, -0.006, 0.0];
            let mut offset = Vec::new();
            for cycle in 0..CYCLES {
                let base = cycle as f64 * step;
                for (index, value) in jitter.iter().enumerate() {
                    let part = base + step * index as f64 / jitter.len() as f64;
                    offset.push((at(part), *value, KeyEase::LINEAR));
                }
            }
            offset.push((at(1.0), 0.0, KeyEase::LINEAR));
            vec![(KeyProperty::OffsetX, offset)]
        }
        // Opacity, slowly, twice: the cheapest way to keep something alive
        // without moving it.
        (Preset::Breathe, _) => {
            const CYCLES: usize = 2;
            let step = 1.0 / CYCLES as f64;
            let mut opacity = Vec::new();
            for cycle in 0..CYCLES {
                let base = cycle as f64 * step;
                opacity.push((at(base), 1.0, KeyEase::IN));
                opacity.push((at(base + step * 0.5), 0.62, KeyEase::IN));
            }
            opacity.push((at(1.0), 1.0, KeyEase::IN));
            vec![(KeyProperty::Opacity, opacity)]
        }
        // A looping preset asked for an arrival, or an arrival asked to loop, is
        // a caller mistake rather than a curve worth inventing. Each falls back
        // to the side it belongs to, so a stray index still animates something
        // sensible instead of nothing.
        (preset, _) if preset.runs_during() => keys(preset, Side::During, span),
        (preset, _) => keys(preset, Side::Enter, span),
    }
}

/// The one command that applies `preset` to `clip` over `seconds`: a batch of
/// keys, so one click is one undo step.
///
/// Only the frames the side owns are rewritten: the keys already there inside
/// that stretch are removed one at a time, and the keys outside it are left
/// exactly as they were. A whole-property clear would be shorter and would also
/// throw away the entrance that was applied a moment before the exit, which is
/// the bug this exists to not have.
pub fn commands(clip: &Clip, preset: Preset, side: Side, seconds: f64) -> Command {
    let seconds = if seconds.is_finite() {
        seconds.clamp(MIN_SECONDS, MAX_SECONDS)
    } else {
        DEFAULT_SECONDS
    };
    let span = if clip.duration > 0.0 {
        seconds / clip.duration
    } else {
        DEFAULT_SECONDS
    };
    let mut batch = Vec::new();
    for (property, keys) in keys(preset, side, span) {
        // Replacing what this side already put there, key by key.
        for key in clip.keys_on(property) {
            if side.owns(key.at) {
                batch.push(Command::ClearClipKey {
                    clip_id: clip.id.clone(),
                    property,
                    at: key.at,
                });
            }
        }
        for (at, value, ease) in keys {
            batch.push(Command::SetClipKey {
                clip_id: clip.id.clone(),
                property,
                at,
                value,
                ease,
            });
        }
    }
    Command::Batch { commands: batch }
}

/// The command that takes an animation off one side of the clip.
///
/// Every key inside that side's own stretch, on every property a preset can
/// write - and nothing outside it. So "none" on the entrance leaves an exit
/// alone, and "none" on the exit leaves the entrance, which is the same rule
/// the presets themselves follow.
pub fn clear(clip: &Clip, side: Side) -> Command {
    let mut batch = Vec::new();
    for property in [
        KeyProperty::Scale,
        KeyProperty::OffsetX,
        KeyProperty::OffsetY,
        KeyProperty::Rotation,
        KeyProperty::Opacity,
    ] {
        for key in clip.keys_on(property) {
            if side.owns(key.at) {
                batch.push(Command::ClearClipKey {
                    clip_id: clip.id.clone(),
                    property,
                    at: key.at,
                });
            }
        }
    }
    Command::Batch { commands: batch }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::model::{ClipKey, ClipKind};

    /// Four seconds, so the default third of a second is an eighth of it and
    /// the arithmetic in the assertions is easy to read.
    fn clip() -> Clip {
        Clip::blank("c1", "t1", ClipKind::Text, "a title", 0.0, 4.0)
    }

    /// The keys a batch writes, by property, in order.
    fn written(command: Command) -> Vec<(KeyProperty, f64, f64)> {
        let Command::Batch { commands } = command else {
            panic!("a preset is a batch");
        };
        commands
            .into_iter()
            .filter_map(|command| match command {
                Command::SetClipKey {
                    property, at, value, ..
                } => Some((property, at, value)),
                _ => None,
            })
            .collect()
    }

    /// The keys a batch removes, in order.
    fn cleared(command: Command) -> Vec<(KeyProperty, f64)> {
        let Command::Batch { commands } = command else {
            panic!("a preset is a batch");
        };
        commands
            .into_iter()
            .filter_map(|command| match command {
                Command::ClearClipKey { property, at, .. } => Some((property, at)),
                _ => None,
            })
            .collect()
    }

    fn enter(preset: Preset) -> Vec<(KeyProperty, f64, f64)> {
        written(commands(&clip(), preset, Side::Enter, DEFAULT_SECONDS))
    }

    fn exit(preset: Preset) -> Vec<(KeyProperty, f64, f64)> {
        written(commands(&clip(), preset, Side::Exit, DEFAULT_SECONDS))
    }

    #[test]
    fn elastic_pop_snaps_past_the_mark_and_settles_back_on_it() {
        let scales: Vec<f64> = enter(Preset::ElasticPop)
            .into_iter()
            .filter(|(property, _, _)| *property == KeyProperty::Scale)
            .map(|(_, _, value)| value)
            .collect();
        assert_eq!(scales.len(), 5, "{scales:?}");
        assert_eq!(scales[0], 0.72, "starts under size");
        assert!(
            scales.iter().any(|value| *value >= 1.12),
            "overshoots far enough to read: {scales:?}"
        );
        assert_eq!(*scales.last().unwrap(), 1.0, "settles on the mark");
    }

    #[test]
    fn the_words_are_there_before_the_figure_has_settled() {
        let keys = enter(Preset::ElasticPop);
        let opaque_at = keys
            .iter()
            .find(|(property, _, value)| *property == KeyProperty::Opacity && *value == 1.0)
            .map(|(_, at, _)| *at)
            .expect("opacity reaches one");
        let settled_at = keys
            .iter()
            .filter(|(property, _, _)| *property == KeyProperty::Scale)
            .map(|(_, at, _)| *at)
            .fold(0.0_f64, f64::max);
        assert!(
            opaque_at < settled_at,
            "opacity at {opaque_at}, scale still moving until {settled_at}"
        );
    }

    #[test]
    fn the_duration_setting_is_the_seconds_it_runs_for() {
        // A quarter of this four-second clip is one second of animation.
        let keys = written(commands(&clip(), Preset::ElasticPop, Side::Enter, 1.0));
        let last = keys.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        assert!((last - 0.25).abs() < 1e-9, "one second of four: {last}");
        // Twice the setting reaches twice as far into the clip.
        let longer = written(commands(&clip(), Preset::ElasticPop, Side::Enter, 1.8));
        let last = longer.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        // 1.8 s is held to MAX_SECONDS first, so what lands is 1.5 of 4.
        assert!((last - 1.5 / 4.0).abs() < 1e-9, "held to the longest: {last}");
        // On a short clip the window itself is the limit: half of two seconds
        // is 0.75, well past the middle, so it is capped there.
        let short = Clip::blank("c2", "t1", ClipKind::Text, "a title", 0.0, 2.0);
        let keys = written(commands(&short, Preset::ElasticPop, Side::Enter, 1.5));
        let last = keys.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        assert!((last - MAX_SPAN).abs() < 1e-9, "capped at the middle: {last}");
        // And an exit of the same length ends on the tail.
        let keys = written(commands(&clip(), Preset::ElasticPop, Side::Exit, 1.0));
        let first = keys.iter().map(|(_, at, _)| *at).fold(1.0_f64, f64::min);
        let last = keys.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        assert!((first - 0.75).abs() < 1e-9, "one second before the tail: {first}");
        assert_eq!(last, 1.0, "ends on the tail");
    }

    /// The bug this module was rewritten for: the exit was clearing the
    /// entrance that had just been applied, because it cleared the whole
    /// property instead of its own stretch of the clip.
    #[test]
    fn applying_an_exit_leaves_the_entrance_alone() {
        let mut keyed = clip();
        keyed.keys.push(ClipKey {
            property: KeyProperty::Scale,
            at: 0.0875,
            value: 0.72,
            ease: KeyEase::OUT,
        });
        let cleared = cleared(commands(&keyed, Preset::ElasticPop, Side::Exit, DEFAULT_SECONDS));
        let cleared_scale: Vec<f64> = cleared
            .iter()
            .filter(|(property, _)| *property == KeyProperty::Scale)
            .map(|(_, at)| *at)
            .collect();
        assert!(
            cleared_scale.is_empty(),
            "the entrance key at 0.0875 must survive an exit: {cleared_scale:?}"
        );
        assert_eq!(cleared.len(), 0, "nothing in the tail to clear: {cleared:?}");
    }

    #[test]
    fn applying_the_same_side_twice_replaces_what_it_wrote() {
        let mut keyed = clip();
        keyed.keys.push(ClipKey {
            property: KeyProperty::Scale,
            at: 0.0875,
            value: 0.72,
            ease: KeyEase::OUT,
        });
        keyed.keys.push(ClipKey {
            property: KeyProperty::Scale,
            at: 0.5,
            value: 1.0,
            ease: KeyEase::OUT,
        });
        let cleared = cleared(commands(&keyed, Preset::Zoom, Side::Enter, DEFAULT_SECONDS));
        let ats: Vec<f64> = cleared
            .iter()
            .filter(|(property, _)| *property == KeyProperty::Scale)
            .map(|(_, at)| *at)
            .collect();
        assert_eq!(ats, vec![0.0875], "only the head key is replaced: {ats:?}");
    }

    #[test]
    fn a_during_preset_loops_inside_the_middle_and_stops_short_of_the_ends() {
        let keys = written(commands(&clip(), Preset::Pulse, Side::During, DEFAULT_SECONDS));
        assert!(keys.len() >= 7, "three cycles: {keys:?}");
        for (property, at, _) in &keys {
            assert_eq!(*property, KeyProperty::Scale);
            assert!(
                (DURING_FROM - 1e-9..=DURING_TO + 1e-9).contains(at),
                "inside the middle: {at}"
            );
        }
        let peaks = keys.iter().filter(|(_, _, value)| *value > 1.0).count();
        assert_eq!(peaks, 3, "one heartbeat per cycle: {peaks}");
        assert_eq!(keys.first().map(|(_, _, v)| *v), Some(1.0));
        assert_eq!(keys.last().map(|(_, _, v)| *v), Some(1.0));
    }

    #[test]
    fn every_preset_keeps_its_keys_inside_the_clip_and_its_own_half() {
        for preset in Preset::ENTER_EXIT {
            for side in [Side::Enter, Side::Exit] {
                for (property, at, _) in written(commands(&clip(), preset, side, DEFAULT_SECONDS)) {
                    assert!(
                        (0.0..=1.0).contains(&at),
                        "{preset:?} {side:?} {property:?} at {at}"
                    );
                    assert!(
                        side.owns(at),
                        "{preset:?} {side:?} writes outside its half: {at}"
                    );
                }
            }
        }
        for preset in Preset::DURING {
            for (_, at, _) in written(commands(&clip(), preset, Side::During, DEFAULT_SECONDS)) {
                assert!(Side::During.owns(at), "{preset:?} at {at}");
            }
        }
    }

    #[test]
    fn every_preset_writes_something_and_returns_to_where_it_started() {
        for preset in Preset::ALL {
            let side = if preset.runs_during() {
                Side::During
            } else {
                Side::Enter
            };
            let keys = written(commands(&clip(), preset, side, DEFAULT_SECONDS));
            assert!(!keys.is_empty(), "{preset:?} writes nothing");
            // Whatever it does, it has to hand the clip back unchanged at the
            // end of its window: identity values only.
            let last_of = |property: KeyProperty| {
                keys.iter()
                    .filter(|(p, _, _)| *p == property)
                    .next_back()
                    .map(|(_, _, value)| *value)
            };
            if let Some(value) = last_of(KeyProperty::Scale) {
                assert!((value - 1.0).abs() < 1e-9, "{preset:?} leaves scale at {value}");
            }
            if let Some(value) = last_of(KeyProperty::Rotation) {
                assert!(
                    value.abs() < 1e-9 || (value.abs() - QUARTER_TURN).abs() < 1e-9,
                    "{preset:?} leaves rotation at {value}"
                );
            }
        }
    }

    #[test]
    fn none_clears_its_own_side_and_leaves_the_others() {
        let mut keyed = clip();
        for (property, at, value) in [
            (KeyProperty::Scale, 0.05, 0.72),
            (KeyProperty::Opacity, 0.1, 0.0),
            (KeyProperty::Scale, 0.5, 1.0),
            (KeyProperty::OffsetY, 0.95, 0.2),
        ] {
            keyed.keys.push(ClipKey {
                property,
                at,
                value,
                ease: KeyEase::OUT,
            });
        }
        let Command::Batch { commands } = clear(&keyed, Side::Enter) else {
            panic!("a batch");
        };
        let ats: Vec<f64> = commands
            .iter()
            .filter_map(|command| match command {
                Command::ClearClipKey { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(ats.len(), 2, "the two keys before the middle: {ats:?}");
        assert!(ats.iter().all(|at| *at < 0.5), "{ats:?}");

        let Command::Batch { commands } = clear(&keyed, Side::During) else {
            panic!("a batch");
        };
        let ats: Vec<f64> = commands
            .iter()
            .filter_map(|command| match command {
                Command::ClearClipKey { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(ats, vec![0.5], "the middle only");
    }

    #[test]
    fn button_indexes_round_trip() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_index(preset.index()), Some(preset));
        }
        assert_eq!(Preset::from_index(13), None);
        assert_eq!(Side::from_index(0), Some(Side::Enter));
        assert_eq!(Side::from_index(1), Some(Side::During));
        assert_eq!(Side::from_index(2), Some(Side::Exit));
        assert_eq!(Side::from_index(3), None);
        for side in [Side::Enter, Side::During, Side::Exit] {
            assert_eq!(Side::from_index(side.index()), Some(side));
        }
    }
}
