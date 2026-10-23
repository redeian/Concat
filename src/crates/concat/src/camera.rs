// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Punching the camera in and out: the two things a screencast actually needs.
//!
//! The camera itself is a layer whose transform moves (see `concat-render`'s
//! `camera`), which means a zoom is keys on scale and offsets like any other
//! animation. What that does not give you on its own is the shape of the
//! gesture, and the gesture is the whole thing:
//!
//! **hold, move, hold.** A move takes a third of a second; a hold lasts as long
//! as you leave it. One key at each end of a ten-second clip - a plain
//! interpolation - is the Ken Burns shape: the picture is never still, and the
//! text is never stable long enough to read. That is the opposite of what a
//! tutorial wants, which is a still frame, a quick reframe, and another still
//! frame. So a punch writes three keys, not two: the value it starts from, the
//! value it lands on after the move, and the same value again at the end of the
//! clip so the camera stays put once it has arrived.
//!
//! **It goes towards something.** Zooming about the frame centre is a lens;
//! zooming about the sentence being explained is an edit. [`Punch::focus`] is
//! that point, in frame fractions, and `concat_render::camera::focus_offsets`
//! turns it into the offsets the clip already knows how to carry - the same
//! arithmetic the compositor uses to hold it still, so what you set is what you
//! see.

use concat_project::commands::Command;
use concat_project::model::{Clip, KeyEase, KeyProperty};
use concat_render::camera::focus_offsets;

/// How long a reframe takes, in seconds. A third of a second is long enough to
/// follow and short enough to feel like a cut with a reason.
pub const MOVE_SECONDS: f64 = 0.3;

/// The zoom levels offered, in the order the panel lists them. Two is a close
/// look; past two and a half a 1080p recording is showing interpolated guesses
/// instead of characters, which is why the list stops at four and warns above
/// two.
#[allow(dead_code)] // The panel's own list; nothing outside it reads it yet.
pub const ZOOMS: [f64; 3] = [1.5, 2.0, 4.0];

/// Where the camera is pointed, and how far in.
#[derive(Clone, Copy, Debug)]
pub struct Punch {
    /// The point to hold still, in frame fractions from the top left.
    pub focus: (f64, f64),
    /// How far in. 1.0 is the whole frame.
    pub scale: f64,
}

impl Punch {
    /// Held to what the panel offers: never below the frame, never past the cap.
    pub fn clamped(self) -> Punch {
        Punch {
            focus: (self.focus.0.clamp(0.0, 1.0), self.focus.1.clamp(0.0, 1.0)),
            scale: self.scale.clamp(1.0, 4.0),
        }
    }
}

/// Which way the punch goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// Arrive at the punch, and stay there.
    In,
    /// Leave it, and stay out.
    Out,
}

impl Side {
    pub fn from_index(index: i32) -> Option<Side> {
        match index {
            0 => Some(Side::In),
            1 => Some(Side::Out),
            _ => None,
        }
    }
}

/// Where in the clip a key goes, as a fraction, held to the clip's ends.
fn at(playhead: f64, offset: f64, duration: f64, move_seconds: f64) -> f64 {
    let move_seconds = move_seconds.clamp(MIN_MOVE_SECONDS, MAX_MOVE_SECONDS);
    let span = if duration > 0.0 {
        move_seconds / duration
    } else {
        move_seconds
    };
    (playhead + offset * span).clamp(0.0, 1.0)
}

/// How short and how long a reframe may take. Below the shortest it is a jump
/// cut; above the longest the camera is drifting, which is the Ken Burns shape
/// this exists to avoid.
pub const MIN_MOVE_SECONDS: f64 = 0.1;
pub const MAX_MOVE_SECONDS: f64 = 1.0;

/// The command that punches at `playhead` - the playhead's place in the clip,
/// `0..=1` - writing the hold, the move and the hold.
///
/// A punch in leaves the camera in; a punch out leaves it out. Both end with a
/// key at the end of the clip so the camera does not drift back to where it
/// started, which is what a single pair of keys would do.
#[allow(dead_code)] // The default-timed punch; the panel times its own.
pub fn commands(clip: &Clip, punch: Punch, side: Side, playhead: f64) -> Command {
    commands_with_move(clip, punch, side, playhead, MOVE_SECONDS)
}

/// The same punch, with the move timed by the caller - the knob in the panel,
/// which is remembered with the other preferences.
pub fn commands_with_move(
    clip: &Clip,
    punch: Punch,
    side: Side,
    playhead: f64,
    move_seconds: f64,
) -> Command {
    let punch = punch.clamped();
    let playhead = playhead.clamp(0.0, 1.0);
    let duration = clip.duration;
    let (offset_x, offset_y) = focus_offsets(punch.scale, punch.focus);
    let (from, to, ease) = match side {
        Side::In => (
            (1.0, 0.0, 0.0),
            (punch.scale, offset_x, offset_y),
            KeyEase::OUT,
        ),
        Side::Out => (
            (punch.scale, offset_x, offset_y),
            (1.0, 0.0, 0.0),
            KeyEase::IN,
        ),
    };
    let start = at(playhead, 0.0, duration, move_seconds);
    let moved = at(playhead, 1.0, duration, move_seconds);
    let mut batch = Vec::new();
    let mut key = |property: KeyProperty, at: f64, value: f64, ease: KeyEase| {
        batch.push(Command::SetClipKey {
            clip_id: clip.id.clone(),
            property,
            at,
            value,
            ease,
        });
    };
    // The hold before the move, the value it lands on, and the hold after.
    key(KeyProperty::Scale, start, from.0, ease);
    key(KeyProperty::Scale, moved, to.0, ease);
    key(KeyProperty::Scale, 1.0, to.0, KeyEase::LINEAR);
    // The offsets ride the same three keys whichever way the punch goes:
    // `from` and `to` already carry the direction.
    key(KeyProperty::OffsetX, start, from.1, ease);
    key(KeyProperty::OffsetX, moved, to.1, ease);
    key(KeyProperty::OffsetX, 1.0, to.1, KeyEase::LINEAR);
    key(KeyProperty::OffsetY, start, from.2, ease);
    key(KeyProperty::OffsetY, moved, to.2, ease);
    key(KeyProperty::OffsetY, 1.0, to.2, KeyEase::LINEAR);
    Command::Batch { commands: batch }
}

/// The command that puts the camera back: every property it writes, cleared.
///
/// A whole-property clear is right here and nowhere else in CapyCut: a camera
/// layer exists to hold the camera, so "reset" means the layer goes back to
/// showing the frame as it is, not "keep whatever else was keyed on it".
pub fn reset(clip: &Clip) -> Command {
    let mut batch = Vec::new();
    for property in [
        KeyProperty::Scale,
        KeyProperty::OffsetX,
        KeyProperty::OffsetY,
    ] {
        if clip.keys_on(property).next().is_some() {
            batch.push(Command::ClearClipKeys {
                clip_id: clip.id.clone(),
                property,
            });
        }
    }
    Command::Batch { commands: batch }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::model::ClipKind;

    fn layer() -> Clip {
        Clip::blank("c1", "t1", ClipKind::Layer, "camera", 0.0, 10.0)
    }

    fn written(command: Command) -> Vec<(KeyProperty, f64, f64)> {
        let Command::Batch { commands } = command else {
            panic!("a punch is a batch");
        };
        commands
            .into_iter()
            .filter_map(|command| match command {
                Command::SetClipKey {
                    property,
                    at,
                    value,
                    ..
                } => Some((property, at, value)),
                _ => None,
            })
            .collect()
    }

    fn of(keys: &[(KeyProperty, f64, f64)], property: KeyProperty) -> Vec<(f64, f64)> {
        keys.iter()
            .filter(|(p, _, _)| *p == property)
            .map(|(_, at, value)| (*at, *value))
            .collect()
    }

    #[test]
    fn a_punch_in_holds_moves_and_holds() {
        let keys = written(commands(
            &layer(),
            Punch {
                focus: (0.5, 0.5),
                scale: 2.0,
            },
            Side::In,
            0.2,
        ));
        let scale = of(&keys, KeyProperty::Scale);
        assert_eq!(scale.len(), 3, "{scale:?}");
        // Hold at one, then the move, then the same value at the end of the
        // clip: the camera does not drift back.
        assert_eq!(scale[0], (0.2, 1.0));
        assert!((scale[1].0 - (0.2 + MOVE_SECONDS / 10.0)).abs() < 1e-9);
        assert_eq!(scale[1].1, 2.0);
        assert_eq!(scale[2], (1.0, 2.0));
    }

    #[test]
    fn a_punch_out_returns_and_stays_out() {
        let keys = written(commands(
            &layer(),
            Punch {
                focus: (0.5, 0.5),
                scale: 2.0,
            },
            Side::Out,
            0.6,
        ));
        let scale = of(&keys, KeyProperty::Scale);
        assert_eq!(scale[0].1, 2.0, "starts where the punch left it");
        assert_eq!(scale[1].1, 1.0);
        assert_eq!(scale[2], (1.0, 1.0), "and stays out");
    }

    #[test]
    fn a_punch_towards_a_corner_offsets_the_frame_the_other_way() {
        // Focus in the top-left quarter: holding it still means pushing the
        // picture right and down, so both offsets are positive.
        let keys = written(commands(
            &layer(),
            Punch {
                focus: (0.25, 0.25),
                scale: 2.0,
            },
            Side::In,
            0.0,
        ));
        let x = of(&keys, KeyProperty::OffsetX);
        let y = of(&keys, KeyProperty::OffsetY);
        assert!(x[1].1 > 0.0, "x offset: {x:?}");
        assert!(y[1].1 > 0.0, "y offset: {y:?}");
        // The point is held: `0.5 + (focus - 0.5) * scale + offset` is focus.
        let held = 0.5 + (0.25 - 0.5) * 2.0 + x[1].1;
        assert!((held - 0.25).abs() < 1e-9, "held at {held}");
    }

    #[test]
    fn the_move_never_leaves_the_clip() {
        // A punch at the very end has nowhere to move to, and must not write a
        // key past the clip: the keys pile up on the last frame instead.
        let keys = written(commands(
            &layer(),
            Punch {
                focus: (0.5, 0.5),
                scale: 1.5,
            },
            Side::In,
            1.0,
        ));
        for (_, at, _) in &keys {
            assert!((0.0..=1.0).contains(at), "at {at}");
        }
    }

    #[test]
    fn the_zoom_and_the_focus_are_held_to_what_the_panel_offers() {
        let punch = Punch {
            focus: (-1.0, 4.0),
            scale: 40.0,
        }
        .clamped();
        assert_eq!(punch.focus, (0.0, 1.0));
        assert_eq!(punch.scale, 4.0);
        let punch = Punch {
            focus: (0.5, 0.5),
            scale: 0.1,
        }
        .clamped();
        assert_eq!(punch.scale, 1.0, "a camera never zooms out past the frame");
    }

    #[test]
    fn reset_clears_the_three_properties_the_camera_writes() {
        let mut keyed = layer();
        keyed.keys.push(concat_project::model::ClipKey {
            property: KeyProperty::Scale,
            at: 0.5,
            value: 2.0,
            ease: KeyEase::LINEAR,
        });
        let Command::Batch { commands } = reset(&keyed) else {
            panic!("a batch");
        };
        assert_eq!(commands.len(), 1, "{commands:?}");
        assert!(matches!(
            commands[0],
            Command::ClearClipKeys {
                property: KeyProperty::Scale,
                ..
            }
        ));
    }
}
