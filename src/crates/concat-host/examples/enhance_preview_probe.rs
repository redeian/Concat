// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! What the Enhance preview actually hands the inspector.
//!
//! The panel showed two white rectangles, which is a claim about pixels rather
//! than about layout, so this runs the same `Enhancers::preview` the button runs
//! and reports what came back: sizes, the mean and range of each channel, and a
//! PPM of each so the picture can be looked at rather than guessed about.
//!
//! ```sh
//! cargo run -p concat-host --example enhance_preview_probe -- \
//!   /path/to/clip.mp4 35.5 2 "$HOME/Library/Application Support/app.capycut.editor" /out/dir
//! ```

use std::path::Path;

use concat_core::Frame;
use concat_host::enhance::{Enhancers, PreviewRequest};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let media = args.next().ok_or("usage: probe <media> <seconds> <factor> <data-dir> <out-dir>")?;
    let seconds: f64 = args.next().unwrap_or_else(|| "0".into()).parse().map_err(|_| "seconds")?;
    let factor: u32 = args.next().unwrap_or_else(|| "2".into()).parse().map_err(|_| "factor")?;
    let data = args.next().ok_or("data dir")?;
    let out = args.next().unwrap_or_else(|| ".".into());

    let service = Enhancers::new(Path::new(&data));
    let request = PreviewRequest {
        media_path: media.clone(),
        seconds,
        factor,
    };
    let started = std::time::Instant::now();
    let preview = service.preview(&request, &mut |_| {})?;
    println!(
        "previewed in {:.1}s · tile {:.3}s · {} tiles a frame",
        started.elapsed().as_secs_f64(),
        preview.tile_seconds,
        preview.tiles_per_frame
    );
    for (name, frame) in [("plain", &preview.plain), ("enhanced", &preview.enhanced)] {
        println!(
            "{name:8} {}x{} {}",
            frame.width(),
            frame.height(),
            describe(frame)
        );
        write_ppm(&Path::new(&out).join(format!("{name}.ppm")), frame)?;
    }
    Ok(())
}

/// Mean, min and max per channel, and how many pixels are pure white. A white
/// rectangle is 255 on every channel of every pixel, which is the shape this is
/// looking for.
fn describe(frame: &Frame) -> String {
    let pixels = frame.pixels();
    let mut sums = [0u64; 4];
    let mut mins = [255u8; 4];
    let mut maxs = [0u8; 4];
    let mut white = 0u64;
    let count = (frame.width() * frame.height()) as u64;
    for pixel in pixels.chunks_exact(4) {
        for channel in 0..4 {
            let value = pixel[channel];
            sums[channel] += u64::from(value);
            mins[channel] = mins[channel].min(value);
            maxs[channel] = maxs[channel].max(value);
        }
        if pixel[..3] == [255, 255, 255] {
            white += 1;
        }
    }
    let mean = |channel: usize| sums[channel] as f64 / count as f64;
    format!(
        "rgba mean [{:.0} {:.0} {:.0} {:.0}] min [{:3} {:3} {:3} {:3}] max [{:3} {:3} {:3} {:3}] white {:.0}%",
        mean(0),
        mean(1),
        mean(2),
        mean(3),
        mins[0],
        mins[1],
        mins[2],
        mins[3],
        maxs[0],
        maxs[1],
        maxs[2],
        maxs[3],
        white as f64 * 100.0 / count.max(1) as f64
    )
}

fn write_ppm(path: &Path, frame: &Frame) -> Result<(), String> {
    let (width, height) = (frame.width(), frame.height());
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in frame.pixels().chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(path, out).map_err(|error| error.to_string())
}
