// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Recolouring a shape after it has been placed.
//!
//! A shape carries its look in [`ShapeStyle`] - fill, outline, outline width,
//! size - and until now that look was chosen once, when the clip was added:
//! there was no way to make a badge match the video it sits on, or to try a
//! colour without deleting the clip and placing it again. This is the command
//! behind the inspector's swatches.
//!
//! Nothing here invents semantics: absent fields keep their value, colours are
//! stored as sent (`#rrggbb` or `#rrggbbaa`, which is what the picker speaks),
//! and a width is floored at zero. A clip that is not a shape is a tolerated
//! no-op rather than an error, the way a patch on a vanished clip is.

use super::*;

/// The fill, the outline, the outline's width and the figure's size, each
/// optional: only what is sent changes.
pub(super) fn apply(
    project: &mut Project,
    _mint: &mut IdMint,
    command: Command,
) -> Result<Outcome, CommandError> {
    let Command::SetShapeStyle {
        clip_id,
        fill,
        stroke,
        stroke_width,
        size,
    } = command
    else {
        unreachable!("commands::apply routes only this module's command here")
    };
    let timeline = project.active_mut();
    let Some(clip) = timeline.clip_mut(&clip_id) else {
        return Ok(Outcome::default());
    };
    let Some(style) = clip.shape.as_mut() else {
        return Ok(Outcome::default());
    };
    let mut applied = false;
    if let Some(fill) = fill {
        applied |= assign(&mut style.fill, fill);
    }
    if let Some(stroke) = stroke {
        applied |= assign(&mut style.stroke, stroke);
    }
    if let Some(width) = stroke_width {
        applied |= assign(&mut style.stroke_width, width.max(0.0));
    }
    if let Some(size) = size {
        applied |= assign(&mut style.size, size.max(0.0));
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
    use crate::model::ShapeKind;

    /// An editor holding one square shape on the first lane, and its id.
    fn fixture() -> (Editor, String) {
        let mut editor = Editor::new();
        let track_id = editor.project().active().tracks[0].id.clone();
        let clip_id = editor
            .apply(Command::AddShapeClip {
                track_id: Some(track_id),
                above: false,
                start: 0.0,
                style: Some(ShapeStyle::of(ShapeKind::Square)),
                duration: Some(5.0),
                name: "Square".to_owned(),
            })
            .expect("adds the shape")
            .created_id
            .expect("an id");
        (editor, clip_id)
    }

    fn style_of(editor: &Editor, clip_id: &str) -> ShapeStyle {
        editor
            .project()
            .active()
            .clip(clip_id)
            .and_then(|clip| clip.shape.clone())
            .expect("a shape")
    }

    #[test]
    fn a_new_fill_lands_and_leaves_the_outline_alone() {
        let (mut editor, clip_id) = fixture();
        let before = style_of(&editor, &clip_id);
        editor
            .apply(Command::SetShapeStyle {
                clip_id: clip_id.clone(),
                fill: Some("#FF0000".to_owned()),
                stroke: None,
                stroke_width: None,
                size: None,
            })
            .expect("recolours");
        let after = style_of(&editor, &clip_id);
        assert_eq!(after.fill, "#FF0000");
        assert_eq!(after.stroke, before.stroke);
        assert_eq!(after.stroke_width, before.stroke_width);
    }

    #[test]
    fn a_negative_width_is_floored_and_a_non_shape_is_a_no_op() {
        let (mut editor, clip_id) = fixture();
        let outcome = editor
            .apply(Command::SetShapeStyle {
                clip_id: clip_id.clone(),
                fill: None,
                stroke: None,
                stroke_width: Some(-2.0),
                size: None,
            })
            .expect("recolours");
        assert!(outcome.applied);
        assert_eq!(style_of(&editor, &clip_id).stroke_width, 0.0);
        let outcome = editor
            .apply(Command::SetShapeStyle {
                clip_id: "c999".to_owned(),
                fill: Some("#00FF00".to_owned()),
                stroke: None,
                stroke_width: None,
                size: None,
            })
            .expect("a missing clip is tolerated");
        assert!(!outcome.applied);
    }
}
