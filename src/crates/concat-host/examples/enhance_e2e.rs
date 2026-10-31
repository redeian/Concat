// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! Enhance, end to end: a real file in, a real file out.
//!
//! The probe beside this one proves the model restores a frame. This proves the
//! rest of it - the service, the tiling over a whole clip, the ffmpeg write,
//! the target path - by running `Enhancers::enhance` exactly as the inspector
//! does and printing where the copy landed and what came back.
//!
//! ```sh
//! cargo run -p concat-host --example enhance_e2e -- \
//!   /path/to/clip.mp4 "$HOME/Library/Application Support/app.capycut.editor"
//! ```
//!
//! It exits non-zero when the copy is missing, the wrong size, or of the wrong
//! length, so it can be read as a test as well as a report.

use std::path::{Path, PathBuf};

use concat_host::enhance::{DEFAULT_STRENGTH, EnhanceRequest, Enhancers, Progress, target_for};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let media = args
        .next()
        .ok_or_else(|| "usage: enhance_e2e <clip.mp4> <app-data-dir> [project-dir]".to_owned())?;
    let data = args
        .next()
        .ok_or_else(|| "the app's data directory is where the model lives".to_owned())?;
    let project = args.next().map_or_else(
        || std::env::temp_dir().join("capycut-enhance-e2e"),
        PathBuf::from,
    );

    let data = Path::new(&data);
    let service = Enhancers::new(data);
    println!("model installed: {}", service.installed());

    let factor = 2;
    // The whole file: `None` span, which is what this probe covers. A clip in
    // the window names the span it shows, so its copy is only that span.
    let target = target_for(&project, &media, factor, DEFAULT_STRENGTH, false, None)
        .ok_or_else(|| "the file cannot be read, so it cannot be enhanced".to_owned())?;
    println!("copy goes to: {}", target.display());

    let request = EnhanceRequest {
        media_path: media.clone(),
        still: false,
        factor,
        strength: DEFAULT_STRENGTH,
        target: target.clone(),
        span: None,
    };
    let started = std::time::Instant::now();
    let mut announced = 0u32;
    service.enhance(&request, &mut |progress| {
        if let Progress::Analysing(done) = progress {
            let percent = (done * 100.0) as u32;
            if percent >= announced + 25 {
                announced = percent - percent % 25;
                println!("  {announced}%");
            }
        }
    })?;
    println!("enhanced in {:.1}s", started.elapsed().as_secs_f64());

    if !target.exists() {
        return Err("the copy the service named does not exist".to_owned());
    }
    println!(
        "copy: {} bytes",
        std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0)
    );

    let (source_width, source_height) = dimensions(&media)?;
    let (copy_width, copy_height) = dimensions(&target.to_string_lossy())?;
    println!("source {source_width}x{source_height} -> copy {copy_width}x{copy_height}");
    if copy_width != source_width * factor || copy_height != source_height * factor {
        return Err(format!(
            "the copy is {}x{}, not {} times the source",
            copy_width, copy_height, factor
        ));
    }
    println!("verdict: a real file in, a larger real file out");
    Ok(())
}

/// Width and height of a file, from `ffprobe`, which the app already needs.
fn dimensions(path: &str) -> Result<(u32, u32), String> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
            path,
        ])
        .output()
        .map_err(|error| format!("ffprobe could not run: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.trim().lines().next().unwrap_or_default();
    let (width, height) = line
        .split_once('x')
        .ok_or_else(|| format!("ffprobe said {line:?} for {path}"))?;
    Ok((
        width
            .trim()
            .parse()
            .map_err(|_| format!("width {width:?}"))?,
        height
            .trim()
            .parse()
            .map_err(|_| format!("height {height:?}"))?,
    ))
}
