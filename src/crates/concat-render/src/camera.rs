// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! A camera over the stack: zoom and pan applied to a treatment layer.
//!
//! CapyCut. A treatment already exists and already does the hard part - it
//! renders everything composited beneath its track into one picture and hands
//! that picture to its effects (`plan::PlannedTreatment`). A camera is the same
//! pass with one more thing to say: draw that picture *here*, at this scale,
//! rather than over the whole frame. So a zoom is a layer clip, the picture
//! beneath it is the sequence of segments it spans, and the movement across a
//! cut is ordinary interpolation on the layer's own keys - there is no seam to
//! smooth because nothing ends at the cut.
//!
//! Three rules live here, because they are arithmetic rather than policy:
//!
//! - **Identity must be exact.** At scale 1 and no offset the camera is skipped
//!   entirely and the stack is drawn straight through. A resample of a UI
//!   screenshot at 100% softens every character on screen, and "why is my
//!   recording blurry in this editor" is a bug report nobody enjoys.
//! - **Zoom is about a point, not about the middle.** Scaling about the frame
//!   centre is a lens; scaling about the thing being explained is an edit.
//!   [`focus_offsets`] turns "keep this point still" into the transform the
//!   clip already knows how to hold.
//! - **The quad is signed device space.** Corners go to the shader in the same
//!   `-1..1` space the rest of the compositor uses, with `+y` up, so a positive
//!   `offset_y` in the transform - which means "down" in the document - has to
//!   come out negative here. Getting that backwards flips every pan, which is
//!   the sort of thing a test catches and an eye does not.

use concat_core::timeline::Transform;

/// How close to exact a value has to be to count as untouched. Well under a
/// pixel at 8K, and far above the noise of accumulated key arithmetic.
pub const EPSILON: f64 = 1e-6;

/// Whether this transform does nothing at all.
///
/// The caller uses this to skip the pass rather than to draw at scale 1: a
/// skipped pass is pixel-identical to no camera, which is the whole point.
pub fn is_identity(transform: &Transform) -> bool {
    (transform.scale - 1.0).abs() < EPSILON
        && transform.offset_x.abs() < EPSILON
        && transform.offset_y.abs() < EPSILON
        && transform.rotation.abs() < EPSILON
        && (transform.stretch_x - 1.0).abs() < EPSILON
        && (transform.stretch_y - 1.0).abs() < EPSILON
}

/// The offsets that hold a point still while the frame is scaled.
///
/// `focus` is in frame fractions, `0..=1` from the top left, which is what a
/// rectangle dragged on the monitor gives. The clip's own mapping is: a point
/// `p` lands at `0.5 + (p - 0.5) * scale + offset`, so holding `focus` fixed
/// means `offset = (focus - 0.5) * (1 - scale)`. At scale 1 that is zero for
/// every point, which is exactly right.
pub fn focus_offsets(scale: f64, focus: (f64, f64)) -> (f64, f64) {
    let keep = 1.0 - scale;
    ((focus.0 - 0.5) * keep, (focus.1 - 0.5) * keep)
}

/// Where the point `focus` ends up under `transform`, in frame fractions. The
/// inverse of [`focus_offsets`], used by the interface to work out what a drag
/// on the monitor means, and by the tests to prove the pair agree.
pub fn focus_after(transform: &Transform, focus: (f64, f64)) -> (f64, f64) {
    let place = |p: f64, offset: f64| 0.5 + (p - 0.5) * transform.scale + offset;
    (
        place(focus.0, transform.offset_x),
        place(focus.1, transform.offset_y),
    )
}

/// The quad a camera draws its picture into, in the compositor's signed device
/// space: `(centre_x, centre_y, half_extent)`.
///
/// One half-extent rather than two, because a camera scales uniformly - a
/// stretched camera is a transform of the picture, and the picture already has
/// `stretch_x` and `stretch_y` for that.
pub fn quad(transform: &Transform) -> (f32, f32, f32) {
    let scale = transform.scale.max(EPSILON) as f32;
    // `+y` is up here and down in the document, so the sign flips.
    (
        transform.offset_x as f32 * 2.0,
        -(transform.offset_y as f32) * 2.0,
        scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(scale: f64, offset_x: f64, offset_y: f64) -> Transform {
        Transform {
            scale,
            offset_x,
            offset_y,
            ..Transform::default()
        }
    }

    #[test]
    fn a_fresh_transform_is_identity_and_a_camera_is_not() {
        assert!(is_identity(&Transform::default()));
        assert!(is_identity(&camera(1.0, 0.0, 0.0)));
        assert!(!is_identity(&camera(1.5, 0.0, 0.0)));
        assert!(!is_identity(&camera(1.0, 0.1, 0.0)));
        assert!(!is_identity(&camera(1.0, 0.0, -0.1)));
    }

    #[test]
    fn zooming_holds_the_focus_point_still() {
        // Zooming about a corner of the frame is the hard case: the offset has
        // to be large, and it has to be in the right direction.
        let focus = (0.25, 0.75);
        for scale in [1.25, 1.5, 2.0, 4.0] {
            let (ox, oy) = focus_offsets(scale, focus);
            let placed = focus_after(&camera(scale, ox, oy), focus);
            assert!(
                (placed.0 - focus.0).abs() < 1e-9 && (placed.1 - focus.1).abs() < 1e-9,
                "scale {scale} moved the focus to {placed:?}"
            );
        }
    }

    #[test]
    fn at_scale_one_the_offsets_are_zero_for_any_focus() {
        for focus in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (0.13, 0.87)] {
            assert_eq!(focus_offsets(1.0, focus), (0.0, 0.0));
        }
        assert!(is_identity(&camera(
            1.0,
            focus_offsets(1.0, (0.9, 0.1)).0,
            focus_offsets(1.0, (0.9, 0.1)).1
        )));
    }

    #[test]
    fn the_quad_grows_with_the_scale_and_pans_the_right_way() {
        // No camera: the quad is the whole frame.
        assert_eq!(quad(&Transform::default()), (0.0, 0.0, 1.0));
        // Twice the size, still centred: twice the extent.
        let (x, y, half) = quad(&camera(2.0, 0.0, 0.0));
        assert_eq!((x, y), (0.0, 0.0));
        assert!((half - 2.0).abs() < 1e-6);
        // Panned right and down in the document: right is `+x` here, down is
        // `-y`, or every pan would come out upside down.
        let (x, y, _) = quad(&camera(1.0, 0.25, 0.1));
        assert!((x - 0.5).abs() < 1e-6, "x: {x}");
        assert!((y + 0.2).abs() < 1e-6, "y: {y}");
    }
}
