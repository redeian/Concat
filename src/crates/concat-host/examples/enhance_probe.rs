// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Does Enhance actually restore anything?
//!
//! A probe, not a test: it loads the downloaded Real-ESRGAN weights, runs one
//! frame of hard-edged checkerboard through the same code the app runs, and
//! prints what came back beside a plain bicubic enlargement of the same frame.
//! Two numbers matter - the output is the size asked for, and its edges are
//! sharper than the plain one's - because those are what "enhance" means.
//!
//! ```sh
//! cargo run -p concat-host --example enhance_probe -- \
//!   "$HOME/Library/Application Support/app.capycut.editor/cutout-models/realesr-general-x4v3.onnx"
//! ```

use concat_core::Frame;
use concat_vision::enhance::{self as vision, Enhancer, Tile};

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: enhance_probe <model.onnx>".to_owned())?;
    let model = Enhancer::load(std::path::Path::new(&path))?;

    // A checkerboard of eight-pixel squares: hard edges everywhere, which is
    // what a restoration is supposed to keep and what a plain resize softens.
    let (width, height) = (96u32, 96u32);
    let mut pixels = Vec::with_capacity((width * height) as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let light = (x / 8 + y / 8) % 2 == 0;
            let value = if light { 235 } else { 20 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let frame = Frame::from_rgba(width, height, pixels)
        .ok_or_else(|| "the frame's pixels do not fill its size".to_owned())?;

    let region = Tile {
        x: 0,
        y: 0,
        width,
        height,
    };
    let (enhanced, times) = model.enhance_region(&frame, region, 2)?;
    let plain = vision::resize(&frame, enhanced.width(), enhanced.height());

    println!(
        "read {}x{} -> wrote {}x{} in {} tile runs",
        width,
        height,
        enhanced.width(),
        enhanced.height(),
        times.len()
    );
    println!(
        "plain bicubic at the same size: {}x{}",
        plain.width(),
        plain.height()
    );
    let (sharp_model, sharp_plain) = (sharpness(&enhanced), sharpness(&plain));
    println!("edge sharpness: model {sharp_model:.1} · plain {sharp_plain:.1}");
    println!("pixels the model changed: {}", differences(&enhanced, &plain));

    let sized = enhanced.width() == width * 2 && enhanced.height() == height * 2;
    let sharper = sharp_model > sharp_plain;
    let did_something = differences(&enhanced, &plain) > 0;
    println!(
        "verdict: size {} · sharper than plain {} · changed the picture {}",
        if sized { "ok" } else { "WRONG" },
        if sharper { "yes" } else { "no" },
        if did_something { "yes" } else { "no" }
    );
    if sized && sharper && did_something {
        Ok(())
    } else {
        Err("the model did not do what Enhance promises".to_owned())
    }
}

/// Mean absolute difference between neighbouring pixels, over luma. Edges are
/// what a restoration is judged on, and this is the cheapest honest measure of
/// them.
fn sharpness(frame: &Frame) -> f64 {
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let pixels = frame.pixels();
    let luma = |x: usize, y: usize| {
        let at = (y * width + x) * 4;
        u16::from(pixels[at]) + u16::from(pixels[at + 1]) + u16::from(pixels[at + 2])
    };
    let mut total = 0u64;
    let mut count = 0u64;
    for y in 0..height {
        for x in 0..width.saturating_sub(1) {
            total += u64::from(luma(x, y).abs_diff(luma(x + 1, y)));
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    total as f64 / count as f64
}

/// How many pixels differ at all between two frames of the same size.
fn differences(a: &Frame, b: &Frame) -> usize {
    a.pixels()
        .chunks_exact(4)
        .zip(b.pixels().chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count()
}
