//! What is on the pasteboard, when it is something a project can hold.
//!
//! Two things arrive that way: files copied in Finder (their URLs on
//! `public.file-url`), and pictures that have no file at all - a screenshot, a
//! chart copied out of a browser - which are `public.png` data. A paste of
//! either is a media import; text and clips are somebody else's business.
//!
//! Objective-C through `objc2`'s `msg_send!`, as the rest of the platform code
//! here is: `NSPasteboard` is reached by name, so no AppKit binding is needed
//! for four calls.

use std::path::PathBuf;

/// What a pasteboard had that a project can hold.
#[derive(Clone, Debug, PartialEq)]
pub enum Picked {
    /// Files copied in Finder, in the order they were copied.
    Files(Vec<PathBuf>),
    /// A picture with no file behind it, as PNG bytes.
    Png(Vec<u8>),
}

/// What the pasteboard holds, when it is something worth pasting.
pub fn read() -> Option<Picked> {
    let files = files();
    if !files.is_empty() {
        return Some(Picked::Files(files));
    }
    png().map(Picked::Png)
}

/// The files on the pasteboard: `public.file-url` gives the first, and the
/// older `NSFilenamesPboardType` gives all of them, newline separated, which
/// is what Finder still writes for a multi-selection.
#[cfg(target_os = "macos")]
fn files() -> Vec<PathBuf> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    let Some(board) = general() else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = Vec::new();
    for kind in ["NSFilenamesPboardType", "public.file-url"] {
        let name = NSString::from_str(kind);
        let held: *mut NSString = unsafe { msg_send![board, stringForType: &*name] };
        if held.is_null() {
            continue;
        }
        let text = unsafe { Retained::retain(held) }
            .map(|held| held.to_string())
            .unwrap_or_default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let path = match line.strip_prefix("file://") {
                Some(rest) => {
                    let decoded = rest.replace("%20", " ");
                    let decoded = percent_decode(&decoded);
                    PathBuf::from(decoded)
                }
                None => PathBuf::from(line),
            };
            if path.exists() && !found.contains(&path) {
                found.push(path);
            }
        }
        if !found.is_empty() {
            break;
        }
    }
    let _ = std::mem::size_of::<AnyObject>();
    found
}

/// A picture on the pasteboard, re-encoded as PNG when it arrives as TIFF
/// (which is what macOS gives a screenshot).
#[cfg(target_os = "macos")]
fn png() -> Option<Vec<u8>> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2_foundation::{NSData, NSString};

    let board = general()?;
    let name = NSString::from_str("public.png");
    let data: *mut NSData = unsafe { msg_send![board, dataForType: &*name] };
    if !data.is_null() {
        let held = unsafe { Retained::retain(data) }?;
        let bytes = held.to_vec();
        if !bytes.is_empty() {
            return Some(bytes);
        }
    }
    // A screenshot arrives as TIFF; `NSBitmapImageRep` would re-encode it, and
    // the engine's own PNG decoder reads TIFF not at all, so this stays
    // unimplemented rather than pretending: the file case is the common one.
    None
}

/// `NSPasteboard.generalPasteboard`, without an AppKit binding.
#[cfg(target_os = "macos")]
fn general() -> Option<&'static objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;

    let class = AnyClass::get(c"NSPasteboard")?;
    let board: *mut objc2::runtime::AnyObject = unsafe { msg_send![class, generalPasteboard] };
    if board.is_null() {
        return None;
    }
    // The general pasteboard lives for the life of the app.
    Some(unsafe { &*board })
}

/// Everything that is not macOS has no pasteboard here yet: a paste falls
/// through to the clip clipboard, which is what it did before.
#[cfg(not(target_os = "macos"))]
fn files() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
fn png() -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "macos")]
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = |byte: u8| (byte as char).to_digit(16);
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A short, stable name for some bytes: FNV-1a, cut to eight characters, the
/// same rule the cache names use. Deterministic across runs, so the same paste
/// twice is one file.
pub fn digest(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")[..8].to_owned()
}

/// The same for a file, without reading a two-gigabyte recording into memory:
/// its length and the first and last sixty-four kilobytes are what name it,
/// which is also how the mask cache names one.
pub fn digest_file(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let meta = std::fs::metadata(path).ok()?;
    let mut bytes = meta.len().to_le_bytes().to_vec();
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 64 * 1024];
    if let Ok(read) = file.read(&mut head) {
        bytes.extend_from_slice(&head[..read]);
    }
    if file
        .seek(SeekFrom::Start(meta.len().saturating_sub(64 * 1024)))
        .is_ok()
    {
        let mut tail = Vec::with_capacity(64 * 1024);
        if file.take(64 * 1024).read_to_end(&mut tail).is_ok() {
            bytes.extend_from_slice(&tail);
        }
    }
    Some(digest(&bytes))
}
