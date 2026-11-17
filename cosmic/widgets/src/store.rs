// SPDX-License-Identifier: GPL-3.0-or-later
//! The impure side of the layout: widgets.conf on disk (locked read-modify-write, atomic
//! replace), the edit-mode flag and the reload stamp in $XDG_RUNTIME_DIR, and the process
//! helpers behind `--show` / `--reset`.
use std::collections::HashMap;
use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::layout::{self, Over, Place};

const HEADER: &str = "## glasskit-widgets layout. Written by edit mode (glasskit-widgets --edit); safe to edit by hand.\n## Keys per [card]: anchor = left|right, x, y (reference px at 1080p), visible = true|false.\n";

pub fn conf_path() -> Option<PathBuf> {
    crate::conf::dir().map(|d| d.join("widgets.conf"))
}

/// $XDG_RUNTIME_DIR (the temp dir when unset, e.g. in tests).
fn run_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}
fn edit_flag() -> PathBuf {
    run_dir().join("glasskit-widgets-edit")
}
fn reload_stamp() -> PathBuf {
    run_dir().join("glasskit-widgets-reload")
}
fn lock_path() -> PathBuf {
    run_dir().join("glasskit-widgets.lock")
}

pub fn edit_on() -> bool {
    edit_flag().exists()
}

pub fn set_edit(on: bool) -> io::Result<()> {
    if on {
        fs::write(edit_flag(), "")
    } else {
        match fs::remove_file(edit_flag()) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Tell running cards to re-read widgets.conf.
pub fn touch_reload() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let _ = fs::write(reload_stamp(), n.to_string());
}

/// Opaque value that changes whenever `touch_reload` ran.
pub fn reload_token() -> Option<String> {
    fs::read_to_string(reload_stamp()).ok()
}

/// Layout from `path` (missing file = nothing set). Problems go to stderr when `warn`.
pub fn load_from(path: &Path, cards: &[&str], warn: bool) -> HashMap<String, Over> {
    let Ok(text) = fs::read_to_string(path) else { return HashMap::new() };
    let (m, w) = layout::parse(&text, cards);
    if warn {
        for w in w {
            eprintln!("glasskit-widgets: widgets.conf {w}");
        }
    }
    m
}

pub fn load(cards: &[&str], warn: bool) -> HashMap<String, Over> {
    conf_path().map_or_else(HashMap::new, |p| load_from(&p, cards, warn))
}

/// Run `f` holding the writers' lock (flock on a file in the runtime dir; released when the file
/// closes, so a crashed writer cannot leave it stuck). Gives up after ~5 s rather than write unlocked.
fn locked<T>(lock: &Path, f: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let file = File::options().create(true).truncate(false).write(true).open(lock)?;
    let t = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if t.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(15)),
            Err(TryLockError::WouldBlock) => return Err(io::Error::other("widgets.conf is locked by another writer")),
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
    f()
}

/// Locked read-modify-write of `path`: re-reads the file every time, so concurrent writers never
/// lose each other's changes; replaced atomically (tmp file + rename).
pub fn update_at(path: &Path, lock: &Path, f: impl FnOnce(&str) -> String) -> io::Result<()> {
    locked(lock, || {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => HEADER.to_string(),
            Err(e) => return Err(e), // unreadable: never clobber
        };
        let new = f(&text);
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension(format!("conf.tmp{}", std::process::id()));
        fs::write(&tmp, new)?;
        fs::rename(&tmp, path).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    })
}

fn update(f: impl FnOnce(&str) -> String) -> io::Result<()> {
    let path = conf_path().ok_or_else(|| io::Error::other("no config dir"))?;
    update_at(&path, &lock_path(), f)
}

pub fn save_place(card: &str, p: &Place) -> io::Result<()> {
    update(|t| layout::set_keys(t, card, &layout::place_kvs(p)))
}

pub fn set_visible(card: &str, v: bool) -> io::Result<()> {
    update(|t| layout::set_keys(t, card, &[("visible", v.to_string())]))
}

/// `--reset`: back widgets.conf up to widgets.conf.bak and delete it. Ok(false) = there was none.
pub fn reset() -> io::Result<bool> {
    let path = conf_path().ok_or_else(|| io::Error::other("no config dir"))?;
    locked(&lock_path(), || match fs::copy(&path, path.with_extension("conf.bak")) {
        Ok(_) => fs::remove_file(&path).map(|()| true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    })
}

/// Whether a `glasskit-widgets --card NAME` process is alive (scans /proc, std only).
pub fn running(card: &str) -> bool {
    let Ok(rd) = fs::read_dir("/proc") else { return false };
    let me = std::process::id().to_string();
    rd.flatten().filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) && e.file_name().to_string_lossy() != me).any(|e| {
        let Ok(b) = fs::read(e.path().join("cmdline")) else { return false };
        let args: Vec<&[u8]> = b.split(|c| *c == 0).collect();
        args.first().is_some_and(|a| a.ends_with(b"glasskit-widgets")) && args.windows(2).any(|w| w[0] == b"--card" && w[1] == card.as_bytes())
    })
}

/// Start `--card NAME` detached from this terminal.
pub fn launch(card: &str) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    Command::new(std::env::current_exe()?).args(["--card", card]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Side;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gk-widgets-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn concurrent_writers_lose_nothing_and_keep_user_lines() {
        let d = tmp("conc");
        let (path, lock) = (d.join("widgets.conf"), d.join("lock"));
        fs::write(&path, "## mine\n[clock]\nx = 1 # keep\n").unwrap();
        let names = ["clock", "calendar", "system", "pet", "media", "toggles", "storage", "focus"];
        let hs: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let (p, l, n) = (path.clone(), lock.clone(), n.to_string());
                std::thread::spawn(move || {
                    for k in 0..10 {
                        let place = Place { side: Side::Right, x: (i * 10 + k) as f32, y: i as f32, visible: true };
                        update_at(&p, &l, |t| layout::set_keys(t, &n, &layout::place_kvs(&place))).unwrap();
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("## mine\n"));
        let m = load_from(&path, &names, false);
        for (i, n) in names.iter().enumerate() {
            assert_eq!(m[*n].x, Some((i * 10 + 9) as f32), "{n}");
            assert_eq!(m[*n].y, Some(i as f32));
        }
        assert!(text.contains("# keep"));
        assert!(!fs::read_dir(&d).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("tmp")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_file_is_created_with_header() {
        let d = tmp("new");
        let path = d.join("sub/widgets.conf");
        update_at(&path, &d.join("lock"), |t| layout::set_keys(t, "pet", &[("visible", "false".into())])).unwrap();
        let t = fs::read_to_string(&path).unwrap();
        assert!(t.starts_with("## glasskit-widgets layout") && t.ends_with("[pet]\nvisible = false\n"), "{t}");
        let _ = fs::remove_dir_all(&d);
    }
}
