//! Save chat with file: the assistant's conversation about a document, written into its `.pcraft`
//! file when the person turns it on (off by default), so it travels with the file and comes back
//! when the file is opened on another computer.
//!
//! A `.pcraft` file is a ZIP bundle: the conversation is one more entry, `artcraft/assistant.json`
//! (portable form, without this computer's conversation id, see `Memory::to_json`), which the
//! bundle reader doesn't look at. Saving with the option off takes it out again. Other formats
//! (PSD, PNG, JPEG…) are saved as they are, and directory bundles too.
//!
//! The desktop app's writer passes each file it writes through [`hooked`].

use std::sync::{Arc, Mutex, OnceLock};

use photocraft_format::zip::{ZipReader, ZipWriter};
use serde_json::Value;

use super::memory::Shelf;

/// The entry that holds the conversation.
pub const PART: &str = "artcraft/assistant.json";
/// The largest conversation entry read back.
const MAX_PART: usize = 8 << 20;
/// The largest file read whole to find its conversation (opening a file).
const MAX_FILE: u64 = 1 << 30;

static SHELF: OnceLock<Arc<Mutex<Shelf>>> = OnceLock::new();

/// Files the conversation can be saved in.
pub fn can_embed(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pcraft"))
}

/// Lets [`hooked`] see the conversations (once per run, from the assistant's memory).
pub fn install(shelf: Arc<Mutex<Shelf>>) {
    let _ = SHELF.set(shelf);
}

/// The bytes to write for `path`: `bytes` with the conversation the assistant has for it (or
/// without one, the option off). None keeps `bytes` as they are.
pub fn hooked(path: &str, bytes: &[u8]) -> Option<Vec<u8>> {
    if !can_embed(path) {
        return None;
    }
    let chat = SHELF.get()?.lock().ok().and_then(|s| s.chat_for(path));
    embed(bytes, chat.as_ref())
}

/// The conversation saved in the file at `path`, if it has one.
pub fn read(path: &str) -> Option<Value> {
    if !can_embed(path) || std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    read_from(&bytes)
}

fn read_from(bytes: &[u8]) -> Option<Value> {
    let zip = ZipReader::new(bytes).ok()?;
    serde_json::from_slice(&zip.read_by_name(PART, MAX_PART).ok()?).ok()
}

/// The bundle `bytes` with `chat` as its conversation entry (replacing one already there), or
/// without one when `chat` is None. None when nothing changes or `bytes` isn't a ZIP bundle.
pub fn embed(bytes: &[u8], chat: Option<&Value>) -> Option<Vec<u8>> {
    let zip = ZipReader::new(bytes).ok()?;
    let has = zip.find(PART).is_some();
    match chat {
        None if !has => return None,
        Some(c) if has && read_from(bytes).as_ref() == Some(c) => return None,
        _ => {}
    }
    let mut out = ZipWriter::new();
    for e in zip.entries.iter().filter(|e| e.name != PART) {
        out.add(&e.name, &zip.read(e, usize::MAX).ok()?).ok()?;
    }
    if let Some(c) = chat {
        out.add(PART, c.to_string().as_bytes()).ok()?;
    }
    out.finish().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundle() -> Vec<u8> {
        let mut z = ZipWriter::new();
        z.add("manifest.json", b"{\"version\":1}").unwrap();
        z.add("tiles/abc.zst", &[1, 2, 3]).unwrap();
        z.finish().unwrap()
    }

    #[test]
    fn a_chat_goes_into_the_bundle_and_comes_out_again() {
        let chat = json!({"version": 1, "messages": [{"role": "user", "text": "Làm sáng ảnh"}]});
        let with = embed(&bundle(), Some(&chat)).unwrap();
        assert_eq!(read_from(&with), Some(chat.clone()));
        let zip = ZipReader::new(&with).unwrap();
        assert_eq!(zip.read_by_name("manifest.json", 100).unwrap(), b"{\"version\":1}", "the bundle's own entries are kept");
        assert_eq!(zip.read_by_name("tiles/abc.zst", 100).unwrap(), [1, 2, 3]);
        assert_eq!(embed(&with, Some(&chat)), None, "the same conversation again: nothing to do");
        let again = embed(&with, Some(&json!({"messages": []}))).unwrap();
        assert_eq!(ZipReader::new(&again).unwrap().entries.len(), 3, "replaced, not doubled");
        let without = embed(&with, None).unwrap();
        assert_eq!(read_from(&without), None, "with the option off it is taken out");
        assert_eq!(embed(&bundle(), None), None, "nothing to do");
        assert_eq!(embed(b"not a zip", Some(&chat)), None);
        assert!(can_embed("C:/Ảnh/Chân dung.PCRAFT") && !can_embed("a.psd"));
    }
}
