# CapyCut zoom — a camera over the timeline

Design note. Status: **proposal, nothing built yet.**

The ask: zoom into a region of a screencast, across clip boundaries, with
keyframes and interpolation between them — for teaching videos where a
recording is cut into segments and the eye has to be taken to one corner of the
screen for a moment.

## What the engine already gives us

Checked in the source, not assumed:

- **A treatment is already a post-composite pass.** A clip of kind
  `Filter` resolves to a `PlannedTreatment` (`concat-render/src/plan.rs:713`):
  *"its effects run over everything composited beneath its track, and the result
  is blended back over the untreated stack"*. So there is already a point in the
  pipeline where the whole lower stack exists as one picture. That is exactly
  where a camera belongs.
- **The gap:** `PlannedTreatment` carries `track`, `effects` and `strength` —
  and **no transform**. A treatment can recolour the stack but cannot move it.
  That is the one thing to add.
- **The transform vocabulary already exists.** `Clip` has `scale`, `offset_x`,
  `offset_y`, `rotation`, `opacity`; every one of them is keyable
  (`KeyProperty`), with `KeyEase` per key and a curve editor that already draws
  diamonds. Interpolation between keys is not something to write.
- Geometry for scaling a picture about a centre already exists for layers
  (`Geometry::of`, `plan.rs:605`), and is the same arithmetic a camera needs.

## The shape of the answer

**Zoom is a treatment layer whose transform moves, not a new kind of clip.**

- One `Filter` clip, on a layer above the footage, as long as the sequence of
  segments it spans — ten seconds of screen recording, six cuts, one clip.
- The zoom is keys on **Scale** + **OffsetX** + **OffsetY** of that clip. All
  three are existing properties, so the project format does not change, and the
  keys open in the curve editor like any others.
- Because the camera is one clip and the keys live on it, interpolation runs
  straight through the cut points. There is no seam to smooth, because nothing
  ends at a cut.
- Titles and overlays sit on a track **above** the camera layer, so they keep
  their own size while the picture beneath them zooms. This is the difference
  between a tutorial that looks made and one that looks filmed.

Work: a `transform` on `PlannedTreatment` (one field), the draw of the treated
stack through the existing geometry maths, and `camera.rs` in `concat-render`
for the pivot arithmetic — a new file plus a small marked patch, which is inside
the conflict budget.

## Why not put zoom on the clips themselves

Tempting, because a clip already has a scale and offset. It fails on the thing
the feature exists for:

- every cut resets the scale, so the zoom has to be rebuilt on both sides of
  every cut, with keys that have to stay numerically identical or the picture
  jumps;
- trim one clip and its neighbour's keys no longer line up — the seam moves and
  the zoom does not;
- a transition between two clips doubles the transform in the overlap;
- undoing a zoom near a cut touches two clips, so it stops being one undo step.

One camera has none of these. It is also the model every editor that does this
well uses (ScreenFlow, Camtasia, Premiere's Transform effect on an adjustment
layer): the zoom belongs to the timeline, not to a clip.

## The craft rules that matter more than the code

These decide whether the feature is usable, so they belong in the defaults:

1. **Hold, move, hold. Never a slow drift.** A move takes 0.25–0.35 s with
   ease-out into ease-in; a hold lasts at least 1.5 s, or it reads as a nervous
   camera. A single lerp between two distant keys — the "Ken Burns" shape — is
   exactly wrong here: the picture is never still, and text is never stable long
   enough to read. So a "punch in" is four keys, not two: hold at 1.0, a fast
   pair through the move, hold at 1.5.
2. **1.5x is a gentle push, 2x is a close look, past 2.5x is mush.** A 1080p
   screen recording zoomed to 2x is showing you interpolated guesswork instead
   of characters. The honest fix is to record at 1440p or 4K and let the camera
   cut in from there, and the UI should say so.
3. **Identity must be exact.** At scale 1, offset 0, the camera must be a
   straight blit of the stack, or bypass the pass entirely. Any resample of a
   UI screenshot at 100% softens every piece of text on screen — the classic
   "why does my recording look blurry in this editor" complaint.
4. **The focus point is a point, not a centre.** Zooming about the frame centre
   is a lens; zooming about the thing you want to look at is an edit. Keys
   therefore need scale and offset together, which is why offset is in the
   property list above.
5. **Snapping.** The focus point wants to snap to what is on screen — a window,
   a code block, a face cam — because "roughly on the terminal" looks like a
   mistake while "exactly on the terminal" looks deliberate.

## The interface

- A **Camera tool** in the toolbar: drag a rectangle over the monitor at the
  playhead → a camera layer is created (or extended) with the four keys of a
  punch. Drag again elsewhere → another punch. That is the whole interaction for
  the common case.
- The **Animation tab** gains a Zoom group: `Punch in`, `Punch out`, `Hold`,
  `Return`, `Reset`, plus the move-duration knob (default 0.3 s) — same idea as
  the existing preset grid, and the same keys underneath.
- The keys are then ordinary diamonds on the camera's clip: draggable, and
  editable in the curve editor that already exists.
- A "camera" clip is a `Filter` clip, so nothing about tracks or kinds changes;
  the tab renames what it is for.

## Phases

1. **Engine.** `camera.rs` in `concat-render`: pivot maths, identity bypass,
   tests that scale 1 / offset 0 draws the same pixels as no camera at all.
   One field on `PlannedTreatment`, one marked block in the draw path.
2. **Keys.** Camera keys through the existing `SetClipKey`; a `Camera` command
   module in `concat-project` that composes a punch out of four keys in one
   batch (one undo step).
3. **UI.** The tool, the presets, the duration knob, and the focus rectangle on
   the monitor.
4. **Later.** Snap-to-element, follow-a-moving-area, auto-punch on click from
   the recording's cursor track.

## Decisions

Settled:

1. **The camera sits below the titles.** Recommended, and the recommendation is
   not a preference: what a tutorial zooms into is the *screen being explained*.
   Captions, badges and callouts are added afterwards and belong to the frame,
   not to the content — if the camera scaled them too, every zoom would make the
   caption bigger and the words would have to be counter-animated back down.
   Because a treatment only reaches the stack beneath its own track, this
   decision costs nothing in code: it is *where the clip goes*. The tool creates
   a camera directly below the lowest title/callout track, with the whole frame
   as the alternative if it is ever wanted ("Camera above all").
2. **Move duration: 0.3 s by default, easing, and adjustable** on the same knob
   as the other animation durations. The value is remembered with the other
   preferences.
3. **Maximum zoom 4x**, with a warning past 2x — not a block. The warning says
   what the problem is (upscaling a screen recording) rather than only that a
   limit was crossed.
4. **No aspect change for now** (16:9 to 9:16 is out of scope). Note that this
   costs nothing to keep open: keys are stored as fractions of the frame, so a
   keyed camera would survive a change of aspect ratio if it is ever asked for.
   Only a camera keyed in pixels would lock us in, and this design never stores
   pixels.

Still open, and cheap to defer:

- Snap-to-element and follow-a-moving-area (the recording's cursor track makes
  the second one possible later).

