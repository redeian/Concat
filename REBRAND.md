# CapyCut — a fork of Concat

CapyCut is a personal, Thai-first fork of [Concat](https://github.com/jub0t/Concat)
by [@jub0t](https://github.com/jub0t). Same engine, same project files, own name
and own Thai defaults.

Upstream is AGPL-3.0-or-later. CapyCut keeps that licence and keeps upstream's
copyright notices intact. The Concat **name and logo** are not part of the
licence grant; upstream's `TRADEMARK.md` asks a modified build to ship under its
own name, which is what "CapyCut" is for. Attribution stays: this is a fork.

## Rules for every feature added here

These are not style preferences. Upstream is a live project with a single
prolific maintainer; a fork that edits the same lines they do pays for it on
every rebase. So:

1. **New behaviour goes in a new file.** A feature gets its own module, next to
   the upstream one it serves (`silence.rs` beside `peaks.rs`, a new preset
   folder, a new effect package). A new file cannot conflict.
2. **Touch upstream files in the smallest possible place, and mark it.** When a
   new module has to be reachable - a `mod` line, one enum variant, one match
   arm - that is the whole edit, on one line or one small block, prefixed
   `// CapyCut:` so a conflict resolution is mechanical.
3. **Never rename what upstream names.** Crate names, module names, command
   variants, JSON-RPC methods, MCP tool names, the project format. A rename
   conflicts forever; a new sibling does not.
4. **Prefer composing existing commands to adding one.** If a feature can be
   expressed as a `Command::Batch` of commands upstream already has, it is
   free of merge risk and free of undo-stack risk. Add a command only when the
   operation cannot be expressed that way - and say so in a comment.
5. **Never run `cargo fmt` over the workspace.** Upstream's tree is not
   rustfmt-clean against a newer toolchain (nearly forty files differ), so a
   workspace-wide format is a diff over the whole repository for no gain. Format
   the files you wrote, by hand or by review.
6. **Leave the licence headers alone.** Add your own copyright line under
   upstream's; never replace theirs. AGPL asks for it and it costs nothing.

`Remove silence` is the worked example of rules 1, 2 and 4: the analysis is a
new file (`concat-media/src/silence.rs`), and the cut is a new command
(`Command::RemoveSpan`) because deleting a span with ripple-close has no
composition from upstream's commands that keeps the edit in one undo step -
`SplitClips` mints the middle clip's id, and an id nobody knows yet cannot be
named in a `Batch` prepared in advance.

## Branches

| Branch | What it is |
|---|---|
| `main` | A pure mirror of `jub0t/concat:main`. Never commit here. Fast-forward only. Pushed to the fork so GitHub shows it current. |
| `mark` | The work. Rebased onto `main` on every sync. |

`upstream`'s push URL is set to `no_push` so nothing can be pushed to the
project this fork came from by accident.

## Keeping up with upstream, then rebuilding

```sh
~/dev/concat-sync.sh status   # how far behind, dirty?, disk free
~/dev/concat-sync.sh sync     # fetch upstream, ff main, rebase mark, push main
~/dev/concat-sync.sh build    # build the editor
~/dev/concat-sync.sh run      # run it
~/dev/concat-sync.sh all      # sync then build
```

To run it as an application rather than a bare binary - menu bar reads
"CapyCut", Dock icon, double-clickable in `~/Applications`:

```sh
~/dev/capycut-app.sh debug    # wrap the debug build   (fast to build, slow to edit video)
~/dev/capycut-app.sh app      # build the shipping profile, then wrap it (slow to build, fast to run)
open -a CapyCut
```

Run from a terminal, a bare binary reports its own file name as the application
name, which is why the menu bar reads "concat" without the bundle.

The script exports `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_DEV_INCREMENTAL=false`. That is not a preference: a default debug
build of this workspace is tens of gigabytes, and the machine this was set up on
had about 20 GB free. A full first build is roughly ten minutes; a rebuild after
a source change is a few minutes, since only the app crate and its dependents
recompile.

`src/rust-toolchain.toml` pins Rust 1.93.1. rustup fetches it on first build.

## What the rebrand touched, and why it is small

The rule: **rename nothing a merge can see through.** Crate names, module names,
JSON-RPC methods, the MCP server's tools and the `concat.json` project format are
all untouched, so an upstream change to any of them applies cleanly and the fork
keeps working with upstream projects and scripts.

What changed is only what a person reads.

| Where | Change |
|---|---|
| `src/crates/concat/src/brand.rs` | New file. `APP_NAME`, `REPOSITORY`, `UPSTREAM_*`, and `in_text()`. The single source of the name. |
| `src/crates/concat/src/i18n.rs` | `t()` runs a lookup through `brand::in_text`, so all 25 shipped locales read "CapyCut" without one of them being edited. A key is never rewritten, so a missing translation still shows its key. |
| `src/crates/concat/src/lib.rs` | `mod brand;` |
| `src/crates/concat/src/platform.rs` | Window title, and the startup-failure dialog. |
| `src/crates/concat/src/sysinfo.rs` | The application row in the system-facts report. |
| `src/crates/concat/src/panes/start.rs` | Launch screen's repository link (goes to the fork); the folder new projects start in, and the test that pins it. |
| `src/crates/concat/src/build.rs` | Windows `VERSIONINFO` product name and description. |
| `src/crates/concat-host/src/dirs.rs` | `IDENTIFIER`: `app.capycut.editor`. Its own settings, recents, model cache. A machine that ran Concat finds an empty CapyCut, deliberately. |

Two known cosmetics, both accepted rather than fixed:

- Locales that inflect the name keep the inflection's ending after the swap
  (Croatian "Concata" becomes "CapyCuta"). English and Thai are unaffected.
- Log files are still named `concat-<when>.log`, inside CapyCut's own folder.
  Renaming them touches the pruning code and its tests for no user-visible gain.

Not touched yet: the macOS bundle id and app name live in
`.github/workflows/build-app.yml`, which only matters for CI-built installers.
A local build runs `src/target/debug/concat` or the `app` profile binary
directly. Bundle, icon and signing are a separate job.

## Thai

### Title presets

Four presets ship in `src/crates/concat/text-presets/`, as `capycut.thai-*`
folders. Presets are discovered by folder, so adding one is a new directory and
never a merge conflict.

| Id | Face | For |
|---|---|---|
| `capycut.thai-impact` | Kanit Black | The hook. Big, stroked, unmistakable. |
| `capycut.thai-bold` | Kanit Bold | Emphasis in yellow, stroked. |
| `capycut.thai-caption` | Prompt Bold | Captions on a dark plate. |
| `capycut.thai-formal` | Noto Sans Thai | Academic and documentary copy. |

Fonts added to `src/crates/concat-text/fonts/` and registered in
`BASE_FONTS`: Kanit Bold, Kanit Black, Prompt Bold, Prompt SemiBold. All OFL;
notices are in `THIRD_PARTY_NOTICES.md`. Noto Sans Thai was already bundled as
the Thai script fallback, and covers the four rare characters the two display
faces do not.

Every Thai preset sets `lineHeight` around 1.4 rather than the default 1.2.
Thai stacks a tone mark above a vowel above the consonant; at 1.2 the stack
collides with the line above.

### Thai text wraps, by cluster

`concat-text`'s line wrapper (`Setter::wrap`) used `unicode_linebreak`, which
implements UAX #14. That algorithm puts Thai in class SA, "complex context": a
script whose words cannot be found without a dictionary, so it declines to guess
and offers **no break opportunity inside a Thai run**. Thai writes without
spaces between words, so a long Thai title held to a `maxWidth` had no break at
all and ran past the frame instead of wrapping.

`concat-text::breaks` now merges UAX #14's opportunities with Thai cluster
boundaries, and both `wrap` and `words` read from it. A cluster is a character
plus the marks hung off it, so a line never starts with a tone mark, a vowel
written above or below, `ๆ`, or the abbreviation mark, and never ends with a
leading vowel (`เ` at the end of a line reads as a word of its own). A break
inside a number is refused. Using the same function for `words` keeps per-word
reveal effects in step with the lines that were actually laid out.

Two tests pin it: `a_thai_line_never_starts_with_a_mark` and
`a_thai_title_without_spaces_still_wraps`.

What this does **not** do is find Thai word boundaries. A dictionary segmenter
(ICU's `brkitr`, or `libthai`) would break between words rather than clusters,
which is what a professional subtitle tool does. Cluster breaking is accepted
here because it needs no new dependency, never splits a mark from its base, and
matches what Thai caption styles do on the timeline. If Thai word breaking is
wanted later, it replaces `breaks` and nothing else.

### Remove silence

**Where it is: engine + CLI. Not in a menu yet.**

The analysis is `concat-media/src/silence.rs`, a new file: ten-millisecond
windows, each to its RMS level in dBFS, runs of windows under the threshold
reported as silence to cut. A stereo frame counts silent only when every
channel is, as FFmpeg's `silencedetect` has it. `suggest_threshold_db` measures
the material - the histogram of window levels usually has two peaks, the speaker
and the room - instead of asking the editor to guess.

Defaults, picked from what the field ships rather than from taste:

| Tool | Threshold | Minimum | Padding |
|---|---|---|---|
| FFmpeg `silencedetect` defaults | -60 dB | 2 s | none |
| FFmpeg as usually driven | -30 dB | 0.5 s | none |
| FireCut (Premiere) recommends | -30 dBFS (-35 advised) | 750 ms | 250 ms each side |
| VideoCascade API | -30 dB | 0.5 s | none |
| auto-editor default | ~-28 dB | - | 0.2 s |
| **CapyCut** | **-30 dBFS** | **0.5 s of cut** | **0.2 s, 0.3 s left between cuts** |

`min_silence` measures the **cut**, after padding - not the run detected before
it. Measured the other way round, on real footage, the setting promised half a
second and delivered cuts of a tenth: 119 of 200 were under the number beside
the slider.

Run it on a file:

```sh
cargo run -p concat-cli -- silence ~/Movies/recording.mp4
cargo run -p concat-cli -- silence ~/Movies/recording.mp4 --threshold -35 --min-silence 0.8
```

With no `--threshold` it measures the file and says so. Decoding is 8 kHz mono
whatever the file is - 115 MB for an hour instead of 1.4 GB.

Measured on a 24-minute screen recording (1445.8 s): measured threshold
-45 dBFS, 80 cuts, 253 s removed, 17% of the file. At -35 dBFS with an 0.8 s
minimum, 68 cuts and 258 s. Both are sane for talking-head footage; the tighter
setting trades a few more cuts for fewer of them being short.

**What is still missing before it can be clicked**, in order:

1. `Command::RemoveSpan { track_id, start, end, ripple }` in `concat-project` -
   the cut itself. It cannot be composed from upstream's commands: `SplitClips`
   mints the middle clip's id, and an id nobody knows yet cannot be named in a
   `Batch` prepared in advance, so the edit would not be one undo step. Logic in
   a new `commands/span.rs`; the upstream edits are one enum variant and one
   match arm, both marked `// CapyCut:`. Edge cases that must be right: a clip
   inside the span, a span over a clip's head, over its tail, across the middle
   of one, across a whole clip, and keyframes (`rewindow_keys`) plus
   `source_start` against `speed`.
2. The action in `studio.rs`, which is where the menus are built in Rust: the
   timeline's context menu (`MenuRow::Action`, around the row of effects verbs)
   and the Edit bar. CapCut and Resolve put auto-cut-silence on the clip's
   right-click; Premiere and FireCut keep it on a menu.
3. Two or three locale strings, then `python3 scripts/locales.py` - CI checks
   that the string inventory is in sync.

### Auto-captions

`concat-speech/src/transcribe.rs` offers six Whisper models: tiny, base, small,
each with an English-only twin. There is no `medium` and no `large-v3-turbo`;
upstream chose that ceiling deliberately, for CPU speed. For Thai, `small` is
where caption quality stops, and it is not good: Thai Whisper needs at least
`medium`, and `large-v3-turbo` is both better and faster than `medium` on Apple
silicon.

Adding one means: an entry in `KNOWN_MODELS` with its SHA-256, the same entry in
`models/manifest.toml`, and `python3 scripts/models.py --check` passing. The
digests have to come from the real file — a made-up digest fails the download
check on every user's machine.

## Rebranding again, later

`brand::APP_NAME` and the `IDENTIFIER` in `dirs.rs` are the two values to change.
Everything else reads from `APP_NAME`. A new name still needs a new icon:
`assets/concat_logo_512.png` is upstream's logo and must not be used for a
distributed build.
