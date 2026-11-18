// SPDX-License-Identifier: GPL-3.0-or-later
//! Keep RSS at the working set (panel applets run many times, one per output).
//! One arena, and a low-priority thread that every few seconds returns freed heap to the OS and
//! unmaps the never-written pages of our own code and libraries (the kernel re-reads them from
//! the page cache on demand, so only what is really executed stays resident).
use std::time::Duration;

pub fn start() {
    // SAFETY: plain glibc tunables, called before any other thread exists.
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 1);
        libc::mallopt(libc::M_TRIM_THRESHOLD, 64 * 1024);
        libc::mallopt(libc::M_MMAP_THRESHOLD, 64 * 1024);
    }
    let _ = std::thread::Builder::new().name("trim".into()).stack_size(128 * 1024).spawn(|| loop {
        std::thread::sleep(Duration::from_secs(3));
        trim();
    });
}

fn trim() {
    // SAFETY: frees only unused heap.
    unsafe { libc::malloc_trim(0) };
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else { return };
    for l in maps.lines() {
        let mut f = l.split_whitespace();
        let (Some(range), Some(perms), Some(off)) = (f.next(), f.next(), f.next()) else { continue };
        let Some(path) = f.nth(2) else { continue };
        // r-xp is code; an r--p at offset 0 is the plain read-only segment. Later r--p ranges are
        // RELRO (relocated, dirty private pages) and must never be discarded.
        let ok = perms == "r-xp" || (perms == "r--p" && off == "00000000");
        let Some((a, b)) = range.split_once('-') else { continue };
        if !ok || !path.starts_with('/') {
            continue;
        }
        let (Ok(a), Ok(b)) = (usize::from_str_radix(a, 16), usize::from_str_radix(b, 16)) else { continue };
        // SAFETY: file-backed, read-only private mapping that was never written: DONTNEED just
        // drops the PTEs; the next access faults the same bytes back in.
        unsafe { libc::madvise(a as *mut _, b - a, libc::MADV_DONTNEED) };
    }
}
