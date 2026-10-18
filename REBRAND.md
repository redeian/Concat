# CapyCut — a fork of Concat

CapyCut is a personal, Thai-first fork of [Concat](https://github.com/jub0t/Concat)
by [@jub0t](https://github.com/jub0t). Same engine, same project files, own name
and own Thai defaults.

Upstream is AGPL-3.0-or-later. CapyCut keeps that licence and keeps upstream's
copyright notices intact. The Concat **name and logo** are not part of the
licence grant; upstream's `TRADEMARK.md` asks a modified build to ship under its
own name, which is what "CapyCut" is for. Attribution stays: this is a fork.

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

### Known bug: Thai text does not wrap

`concat-text`'s line wrapper (`Setter::wrap`) uses `unicode_linebreak`, which
implements UAX #14. That algorithm treats Thai as a "complex context" script and
offers no break opportunities inside a Thai run, because breaking Thai needs a
dictionary. Thai writes without spaces between words, so a long Thai title with
a `maxWidth` set has **no break opportunity at all** and overflows the frame
instead of wrapping.

The fix is a Thai break pass: insert break opportunities between Thai clusters
(the base character plus its combining marks, minus breaks after a leading vowel
and before the repetition mark `ๆ`), and merge them with the UAX #14 result in
`wrap`. It belongs in `concat-text/src/lib.rs`, in the same commit as a test
that renders a long Thai string at a narrow `maxWidth` and asserts more than one
line.

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
