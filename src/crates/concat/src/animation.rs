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
//! Keys are written as fractions of the clip, so a preset survives a trim: a
//! title cut shorter still pops in over its own first third.

use concat_project::commands::Command;
use concat_project::model::{Clip, KeyEase, KeyProperty};

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

/// The keys a preset writes, per property.
fn keys(preset: Preset, side: Side) -> Vec<(KeyProperty, Vec<Key>)> {
    match (preset, side) {
        // The overshoot is the animation: past the mark, back under it, and a
        // last small correction. Three bounces read as elastic; one reads as a
        // glitch.
        (Preset::ElasticPop, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(0.0, 0.0, KeyEase::OUT), (0.12, 1.0, KeyEase::OUT)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (0.0, 0.55, KeyEase::OUT),
                    (0.35, 1.12, KeyEase::OUT),
                    (0.55, 0.96, KeyEase::OUT),
                    (0.78, 1.03, KeyEase::OUT),
                    (1.0, 1.0, KeyEase::OUT),
                ],
            ),
        ],
        (Preset::ElasticPop, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(0.88, 1.0, KeyEase::IN), (1.0, 0.0, KeyEase::IN)],
            ),
            (
                KeyProperty::Scale,
                vec![
                    (0.0, 1.0, KeyEase::IN),
                    (0.88, 1.0, KeyEase::IN),
                    (0.94, 1.08, KeyEase::IN),
                    (1.0, 0.55, KeyEase::IN),
                ],
            ),
        ],
        (Preset::Fade, Side::Enter) => vec![(
            KeyProperty::Opacity,
            vec![(0.0, 0.0, KeyEase::OUT), (0.15, 1.0, KeyEase::OUT)],
        )],
        (Preset::Fade, Side::Exit) => vec![(
            KeyProperty::Opacity,
            vec![(0.85, 1.0, KeyEase::IN), (1.0, 0.0, KeyEase::IN)],
        )],
        // A fifth of the frame is enough to read as movement on a 1080p cut
        // and small enough not to look like a mistake.
        (Preset::Slide, Side::Enter) => vec![
            (
                KeyProperty::Opacity,
                vec![(0.0, 0.0, KeyEase::OUT), (0.20, 1.0, KeyEase::OUT)],
            ),
            (
                KeyProperty::OffsetY,
                vec![(0.0, 0.18, KeyEase::OUT), (0.35, 0.0, KeyEase::OUT)],
            ),
        ],
        (Preset::Slide, Side::Exit) => vec![
            (
                KeyProperty::Opacity,
                vec![(0.80, 1.0, KeyEase::IN), (1.0, 0.0, KeyEase::IN)],
            ),
            (
                KeyProperty::OffsetY,
                vec![(0.0, 0.0, KeyEase::IN), (0.70, 0.0, KeyEase::IN), (1.0, 0.18, KeyEase::IN)],
            ),
        ],
    }
}

/// The one command that applies `preset` to `clip`: a batch of keys, so one
/// click is one undo step.
///
/// A property the clip already rides is cleared first: a preset replaces what
/// was on that property rather than interleaving with it, which would leave a
/// curve nobody could predict. Properties the preset does not touch are left
/// exactly as they were, so an enter animation does not disturb a hand-made
/// exit.
pub fn commands(clip: &Clip, preset: Preset, side: Side) -> Command {
    let mut batch = Vec::new();
    for (property, keys) in keys(preset, side) {
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

    #[test]
    fn elastic_pop_overshoots_and_settles_back_on_the_mark() {
        let keys = written(commands(&clip(), Preset::ElasticPop, Side::Enter));
        let scales: Vec<f64> = keys
            .iter()
            .filter(|(property, _, _)| *property == KeyProperty::Scale)
            .map(|(_, _, value)| *value)
            .collect();
        assert_eq!(scales.len(), 5, "{scales:?}");
        assert_eq!(scales[0], 0.55, "starts small");
        assert!(scales.iter().any(|value| *value > 1.0), "overshoots");
        assert_eq!(*scales.last().unwrap(), 1.0, "settles on the mark");
    }

    #[test]
    fn every_key_sits_inside_the_clip_and_the_ends_are_touched() {
        for preset in Preset::ALL {
            for side in [Side::Enter, Side::Exit] {
                for (property, at, _) in written(commands(&clip(), preset, side)) {
                    assert!(
                        (0.0..=1.0).contains(&at),
                        "{preset:?} {side:?} {property:?} at {at}"
                    );
                }
            }
        }
        let enter = written(commands(&clip(), Preset::ElasticPop, Side::Enter));
        assert!(enter.iter().any(|(_, at, _)| *at == 0.0), "keys the head");
        let exit = written(commands(&clip(), Preset::ElasticPop, Side::Exit));
        assert!(exit.iter().any(|(_, at, _)| *at == 1.0), "keys the tail");
    }

    #[test]
    fn an_exit_leaves_the_head_alone_and_an_enter_leaves_the_tail_alone() {
        let enter = written(commands(&clip(), Preset::ElasticPop, Side::Enter));
        assert!(enter.iter().all(|(_, at, _)| *at < 1.0), "{enter:?}");
        let exit = written(commands(&clip(), Preset::ElasticPop, Side::Exit));
        assert!(exit.iter().any(|(_, at, _)| *at > 0.5), "{exit:?}");
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
        let Command::Batch { commands } = commands(&keyed, Preset::Fade, Side::Enter) else {
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

        let Command::Batch { commands } = commands(&keyed, Preset::ElasticPop, Side::Enter) else {
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
