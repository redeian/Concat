// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// SPDX-FileCopyrightText: 2026 CapyCut contributors

//! This build's name, and the names it points at elsewhere.
//!
//! CapyCut is a fork of Concat, and it keeps upstream's code, crate names and
//! project files untouched so that `upstream` merges stay cheap. What changes
//! is only what a person reads: the window title, the dialogs, the folder new
//! projects start in, and the word a locale file never has to spell out.
//!
//! [`in_text`] is applied to every string the interface shows, so the shipped
//! locales read "CapyCut" without any of the 25 files being edited - and an
//! upstream translation that arrives later arrives already branded. Keys are
//! never touched, so `start.notConcatProjectFolder` stays a key, and a string
//! nothing translates still reads as its key rather than as a half-renamed
//! one.

/// What this build calls itself, wherever a person reads it.
pub const APP_NAME: &str = "CapyCut";

/// The project this one is a fork of. `TRADEMARK.md` upstream asks a modified
/// build to carry its own name; it keeps this one for attribution. The
/// upstream repository is https://github.com/jub0t/Concat - not a constant
/// here, since nothing in the interface links to it: a person reporting a bug
/// belongs in the fork's issues, where the bug actually is.
pub const UPSTREAM_NAME: &str = "Concat";

/// This fork's repository: code, issues, releases. The launch screen's "the
/// code" link goes here, so a user reporting a bug lands where the bug is, not
/// where it is not.
pub const REPOSITORY: &str = "https://github.com/redeian/Concat";

/// `text` with this build's name in place of the upstream one.
///
/// Called on the result of a locale lookup only, never on a key: a value is
/// words a person reads, and a key is an address. A locale that inflects the
/// upstream name - Croatian "Concata", Slovak "Concatov" - keeps the
/// inflection's ending, which reads oddly and is cosmetic; every language that
/// matters here spells it uninflected.
pub fn in_text(text: String) -> String {
    if text.contains(UPSTREAM_NAME) {
        text.replace(UPSTREAM_NAME, APP_NAME)
    } else {
        text
    }
}
