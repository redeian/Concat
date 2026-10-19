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
//! Two sides, because that is how an edit is described: [`Side::Enter`] keys
//! the head of the clip, [`Side::Exit`] keys the tail. The middle is left
//! alone, so an enter and an exit can both be on one clip.
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
//! seconds - a default of 0.35, a range of 0.15 to 1.5 - and that window is
//! turned into a fraction of the clip here.

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

/// Which end of the clip a preset keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The head: how it arrives.
    Enter,
    /// The tail: how it leaves.
    Exit,
}

/// The animations offered, in the order the inspector shows them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    /// Scale overshoots and settles - the one everybody means by "pop".
    ElasticPop,
    /// Opacity only: the quietest arrival and departure there is.
    Fade,
    /// Comes up from below, or leaves downwards.
    Slide,
}

impl Preset {
    /// Every preset, in the order the panel lists them.
    pub const ALL: [Preset; 3] = [Preset::ElasticPop, Preset::Fade, Preset::Slide];

    /// The index the inspector's buttons send.
    pub fn index(self) -> i32 {
        match self {
            Preset::ElasticPop => 0,
            Preset::Fade => 1,
            Preset::Slide => 2,
        }
    }

    /// The preset a button index names, if any.
    pub fn from_index(index: i32) -> Option<Preset> {
        Preset::ALL.get(usize::try_from(index).ok()?).copied()
    }
}

impl Side {
    /// The side a button names: 0 enter, 1 exit.
    pub fn from_index(index: i32) -> Option<Side> {
        match index {
            0 => Some(Side::Enter),
            1 => Some(Side::Exit),
            _ => None,
        }
    }
}

/// One key: where in the clip, what value, and how it is approached.
type Key = (f64, f64, KeyEase);

/// The keys a preset writes, per property, inside `span` - the share of the
/// clip the animation covers, as a fraction.
fn keys(preset: Preset, side: Side, span: f64) -> Vec<(KeyProperty, Vec<Key>)> {
    // Two per cent at each end, always. Two animations on one clip must not
    // fight over the same frames, and a window of a whole clip is a move, not
    // an entrance.
    let span = span.clamp(0.02, 0.9);
    let at = move |part: f64| match side {
        Side::Enter => (span * part).clamp(0.0, 1.0),
        Side::Exit => (1.0 - span + span * part).clamp(0.0, 1.0),
    };
    match (preset, side) {
        // The overshoot is the animation: small, past the mark, under it, one
        // last correction. Opacity is done a quarter of the way in.
        (Preset::ElasticPop, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, KeyEase::OUT), (at(0.25), 1.0, KeyEase::OUT)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (at(0.0), 0.72, KeyEase::OUT),
                    (at(0.32), 1.15, KeyEase::OUT),
                    (at(0.55), 0.97, KeyEase::OUT),
                    (at(0.78), 1.04, KeyEase::OUT),
                    (at(1.0), 1.0, KeyEase::OUT),
                ],
            ),
        ],
        // The same shape backwards, and shorter: an exit that dawdles holds
        // the cut up.
        (Preset::ElasticPop, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.55), 1.0, KeyEase::IN), (at(1.0), 0.0, KeyEase::IN)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (at(0.0), 1.0, KeyEase::IN),
                    (at(0.45), 1.0, KeyEase::IN),
                    (at(0.62), 1.12, KeyEase::IN),
                    (at(1.0), 0.70, KeyEase::IN),
                ],
            ),
        ],
        (Preset::Fade, Side::Enter) => vec![(
            KeyProperty::Opacity,
            vec![(at(0.0), 0.0, KeyEase::OUT), (at(0.6), 1.0, KeyEase::OUT)],
        )],
        (Preset::Fade, Side::Exit) => vec![(
            KeyProperty::Opacity,
            vec![(at(0.4), 1.0, KeyEase::IN), (at(1.0), 0.0, KeyEase::IN)],
        )],
        // A fifth of the frame is enough to read as movement on a 1080p cut
        // and small enough not to look like a mistake.
        (Preset::Slide, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.0), 0.0, KeyEase::OUT), (at(0.5), 1.0, KeyEase::OUT)],
            ),
            (
                KeyProperty::OffsetY,
                vec![(at(0.0), 0.22, KeyEase::OUT), (at(1.0), 0.0, KeyEase::OUT)],
            ),
        ],
        (Preset::Slide, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(at(0.5), 1.0, KeyEase::IN), (at(1.0), 0.0, KeyEase::IN)],
            ),
            (
                KeyProperty::OffsetY,
                vec![
                    (at(0.0), 0.0, KeyEase::IN),
                    (at(0.45), 0.0, KeyEase::IN),
                    (at(1.0), 0.22, KeyEase::IN),
                ],
            ),
        ],
    }
}

/// The one command that applies `preset` to `clip` over `seconds`: a batch of
/// keys, so one click is one undo step.
///
/// A property the clip already rides is cleared first: a preset replaces what
/// was on that property rather than interleaving with it, which would leave a
/// curve nobody could predict. Properties the preset does not touch are left
/// exactly as they were, so an enter animation does not disturb a hand-made
/// exit.
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
        if clip.keys_on(property).next().is_some() {
            batch.push(Command::ClearClipKeys {
                clip_id: clip.id.clone(),
                property,
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::model::ClipKind;

    /// Four seconds, so a default half-second animation is an eighth of it and
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
        let longer = written(commands(&clip(), Preset::ElasticPop, Side::Enter, 2.0));
        let last = longer.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        assert!((last - 0.5).abs() < 1e-9, "two seconds of four: {last}");
        // And an exit of the same length starts where the enter ends.
        let keys = written(commands(&clip(), Preset::ElasticPop, Side::Exit, 1.0));
        let first = keys.iter().map(|(_, at, _)| *at).fold(1.0_f64, f64::min);
        assert!((first - 0.75).abs() < 1e-9, "one second before the tail: {first}");
    }

    #[test]
    fn every_key_sits_inside_the_clip_and_the_ends_are_touched() {
        for preset in Preset::ALL {
            for side in [Side::Enter, Side::Exit] {
                for (property, at, _) in written(commands(&clip(), preset, side, DEFAULT_SECONDS)) {
                    assert!(
                        (0.0..=1.0).contains(&at),
                        "{preset:?} {side:?} {property:?} at {at}"
                    );
                }
            }
        }
        assert!(
            enter(Preset::ElasticPop).iter().any(|(_, at, _)| *at == 0.0),
            "keys the head"
        );
        assert!(
            exit(Preset::ElasticPop).iter().any(|(_, at, _)| *at == 1.0),
            "keys the tail"
        );
    }

    #[test]
    fn an_exit_leaves_the_head_alone_and_an_enter_leaves_the_tail_alone() {
        let keys = enter(Preset::ElasticPop);
        let last = keys.iter().map(|(_, at, _)| *at).fold(0.0_f64, f64::max);
        assert!(last < 0.5, "an enter stays near the head: {last}");
        let keys = exit(Preset::ElasticPop);
        let first = keys.iter().map(|(_, at, _)| *at).fold(1.0_f64, f64::min);
        assert!(first > 0.5, "an exit stays near the tail: {first}");
    }

    #[test]
    fn a_property_that_already_rides_is_cleared_before_the_preset_writes_it() {
        let mut keyed = clip();
        keyed.keys.push(concat_project::model::ClipKey {
            property: KeyProperty::Scale,
            at: 0.5,
            value: 2.0,
            ease: KeyEase::LINEAR,
        });
        let Command::Batch { commands } =
            commands(&keyed, Preset::Fade, Side::Enter, DEFAULT_SECONDS)
        else {
            panic!("a batch");
        };
        // Fade writes opacity only, so nothing is cleared: a preset must not
        // throw away an animation it is not replacing.
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::ClearClipKeys { .. })),
            "{commands:?}"
        );

        let Command::Batch { commands } =
            commands(&keyed, Preset::ElasticPop, Side::Enter, DEFAULT_SECONDS)
        else {
            panic!("a batch");
        };
        assert!(
            matches!(
                commands.first(),
                Some(Command::ClearClipKeys {
                    property: KeyProperty::Scale,
                    ..
                })
            ),
            "the keyed property is cleared first: {commands:?}"
        );
    }

    #[test]
    fn button_indexes_round_trip() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_index(preset.index()), Some(preset));
        }
        assert_eq!(Preset::from_index(9), None);
        assert_eq!(Side::from_index(0), Some(Side::Enter));
        assert_eq!(Side::from_index(1), Some(Side::Exit));
        assert_eq!(Side::from_index(2), None);
    }
}
