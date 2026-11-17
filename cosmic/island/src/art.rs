// SPDX-License-Identifier: GPL-3.0-or-later
//! Album art for the popup card. `ember_core`'s `Player` has only title/artist/playing, so the art URL is
//! read straight from the same MPRIS player (zbus). The loader is a copy of the desktop Media card's
//! (`widgets/src/mediacard.rs`): same security checks, same cache dir (`~/.cache/glasskit/art`), so a
//! cover is fetched once for both.
use std::collections::HashMap;

use zbus::proxy::{Builder, CacheProperties};
use zbus::{OwnedVariant, Variant};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE: &str = "org.mpris.MediaPlayer2.Player";
/// Thumbnail edge: the tile is 64 logical px, so 128 covers 2x scaling.
const THUMB_PX: usize = 128;

/// String or object path -> String.
fn string(v: &Variant) -> Option<String> {
    match v {
        Variant::Str(s) => Some(s.as_str().to_owned()),
        Variant::ObjectPath(p) => Some(p.as_str().to_owned()),
        Variant::Variant(b) => string(b),
        _ => None,
    }
}

/// `mpris:artUrl` out of a Metadata dict; empty is "none".
pub fn parse_art_url(m: &HashMap<String, OwnedVariant>) -> Option<String> {
    m.get("mpris:artUrl").and_then(|v| string(v)).filter(|s| !s.is_empty())
}

/// The art URL of the player on `bus` (a bus name `media::read` picked), if it has one.
pub async fn read_art_url(bus: &str) -> Option<String> {
    if !bus.starts_with(PREFIX) || bus.contains('/') {
        return None;
    }
    let conn = zbus::Connection::session().await.ok()?;
    let p: zbus::Proxy<'static> = Builder::new(&conn).destination(bus.to_owned()).path(PATH).interface(IFACE).cache_properties(CacheProperties::No).build().await.ok()?;
    let meta: HashMap<String, OwnedVariant> = p.get_property("Metadata").await.ok()?;
    parse_art_url(&meta)
}

// ---------------------------------------------------------------- album art (copied from widgets/src/mediacard.rs)

/// `file:///a%20b.png` -> `/a b.png`. http(s) (or anything else) -> None (remote art goes through `fetch_cached`).
pub fn art_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(v) = rest.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).ok().map(Into::into)
}

/// Centre-crop to a square and box-average down to at most `max` px. RGBA in, RGBA out.
pub fn square_thumb(w: usize, h: usize, rgba: &[u8], max: usize) -> Option<(u32, Vec<u8>)> {
    let s = w.min(h);
    if s == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let n = s.min(max);
    let (x0, y0) = ((w - s) / 2, (h - s) / 2);
    let mut out = Vec::with_capacity(n * n * 4);
    for j in 0..n {
        let (ya, yb) = (j * s / n, ((j + 1) * s / n).max(j * s / n + 1));
        for i in 0..n {
            let (xa, xb) = (i * s / n, ((i + 1) * s / n).max(i * s / n + 1));
            let mut acc = [0u32; 4];
            for y in ya..yb {
                for x in xa..xb {
                    let o = ((y0 + y) * w + x0 + x) * 4;
                    for c in 0..4 {
                        acc[c] += u32::from(rgba[o + c]);
                    }
                }
            }
            let k = ((yb - ya) * (xb - xa)) as u32;
            out.extend(acc.map(|a| (a / k) as u8));
        }
    }
    Some((n as u32, out))
}

fn decode_png(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(data));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let f = r.next_frame(&mut buf).ok()?;
    let px = &buf[..f.buffer_size()];
    let rgba = match f.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((f.width as usize, f.height as usize, rgba))
}

fn decode_jpeg(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut d = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(data));
    let px = d.decode().ok()?; // RGB
    let i = d.info()?;
    let (w, h) = (i.width as usize, i.height as usize);
    (px.len() >= w * h * 3).then(|| (w, h, px.chunks_exact(3).take(w * h).flat_map(|p| [p[0], p[1], p[2], 255]).collect()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Img {
    Png,
    Jpeg,
}

/// Sniff magic bytes: PNG 89504E47, JPEG FFD8. Anything else is not art.
fn sniff(data: &[u8]) -> Option<Img> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(Img::Png)
    } else if data.starts_with(&[0xff, 0xd8]) {
        Some(Img::Jpeg)
    } else {
        None
    }
}

fn decode_thumb(data: &[u8]) -> Option<cosmic::widget::image::Handle> {
    let (w, h, rgba) = match sniff(data)? {
        Img::Png => decode_png(data)?,
        Img::Jpeg => decode_jpeg(data)?,
    };
    let (n, px) = square_thumb(w, h, &rgba, THUMB_PX)?;
    Some(cosmic::widget::image::Handle::from_rgba(n, n, px))
}

/// Remote art is only fetched from a public DNS name over plain http/https on the default port, so a hostile
/// MPRIS player cannot make the card probe localhost or the LAN. IP literals (any notation: the last label
/// must be alphabetic), `localhost` and local-only suffixes are refused.
/// ponytail: a public name that resolves to a private address (or redirects to one) is not caught; resolve and check if that matters.
pub fn remote_art_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else { return false };
    if url.bytes().any(|c| c <= b' ' || c == 0x7f || c == b'\\') {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let last = labels.last().copied().unwrap_or("");
    labels.len() >= 2
        && labels.iter().all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        && last.bytes().all(|c| c.is_ascii_alphabetic())
        && !matches!(last, "localhost" | "local" | "internal" | "lan" | "home" | "localdomain")
}

/// Stable cache file name for a URL: FNV-1a 64 as 16 hex digits + ".img".
fn cache_name(url: &str) -> String {
    let h = url.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    format!("{h:016x}.img")
}

const CACHE_KEEP: usize = 50;
const MAX_ART_BYTES: u64 = 8_000_000;

/// Which cache files to delete: the oldest ones beyond `keep`. `files` is (name, modified).
fn evict_choice(files: &[(String, std::time::SystemTime)], keep: usize) -> Vec<String> {
    let mut f: Vec<_> = files.iter().collect();
    f.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))); // newest first
    f.iter().skip(keep).map(|(n, _)| n.clone()).collect()
}

fn cache_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).or_else(|| Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join(".cache")))?;
    Some(base.join("glasskit").join("art"))
}

/// `curl` with an argument vector (no shell, URL never interpolated); `-q` skips ~/.curlrc.
fn curl(url: &str, out: &std::path::Path) -> bool {
    use std::process::{Command, Stdio};
    Command::new("curl")
        .args(["-q", "-fsSL", "--max-time", "8", "--max-filesize", "8000000", "--max-redirs", "3", "--proto", "=http,https", "--proto-redir", "=https", "-o"])
        .arg(out)
        .args(["--url", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Bytes of a remote image, from the cache or via `download(url, tmp_path)`. Only PNG/JPEG under 8 MB are
/// kept; the file is written to a tmp name and renamed, then the cache is trimmed to `CACHE_KEEP` files.
fn fetch_cached(url: &str, dir: &std::path::Path, download: impl Fn(&str, &std::path::Path) -> bool) -> Option<Vec<u8>> {
    use std::os::unix::fs::DirBuilderExt;
    if !remote_art_ok(url) {
        return None;
    }
    let (name, file) = (cache_name(url), dir.join(cache_name(url)));
    if let Ok(d) = std::fs::read(&file) {
        if sniff(&d).is_some() {
            return Some(d);
        }
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).ok()?;
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    let data = (|| {
        if !download(url, &tmp) || std::fs::metadata(&tmp).ok()?.len() > MAX_ART_BYTES {
            return None;
        }
        let d = std::fs::read(&tmp).ok()?;
        sniff(&d)?;
        std::fs::rename(&tmp, &file).ok()?;
        Some(d)
    })();
    let _ = std::fs::remove_file(&tmp);
    if data.is_some() {
        let files: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.file_name().into_string().ok()?, e.metadata().ok()?.modified().ok()?)))
            .filter(|(n, _)| n.ends_with(".img"))
            .collect();
        for n in evict_choice(&files, CACHE_KEEP) {
            let _ = std::fs::remove_file(dir.join(n));
        }
    }
    data
}

/// Remote URLs that already failed this session: each is tried once, never again.
fn failed() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static F: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    F.get_or_init(Default::default)
}

/// Decode a local (file://) or remote (http/https via curl, cached) PNG/JPEG into a 256px square thumbnail.
pub async fn load_art(url: String) -> Option<cosmic::widget::image::Handle> {
    tokio::task::spawn_blocking(move || {
        if let Some(path) = art_path(&url) {
            if std::fs::metadata(&path).ok()?.len() > 32 << 20 {
                return None;
            }
            return decode_thumb(&std::fs::read(path).ok()?);
        }
        if failed().lock().ok()?.contains(&url) {
            return None;
        }
        let h = fetch_cached(&url, &cache_dir()?, curl).and_then(|d| decode_thumb(&d));
        if h.is_none() {
            failed().lock().ok()?.insert(url);
        }
        h
    })
    .await
    .ok()
    .flatten()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn ov(v: Variant<'static>) -> OwnedVariant {
        OwnedVariant::try_from(v).unwrap()
    }

    #[test]
    fn art_url_from_metadata() {
        let mut m = HashMap::new();
        assert_eq!(parse_art_url(&m), None);
        m.insert("mpris:artUrl".to_string(), ov(Variant::Str("https://i.scdn.co/image/abc".into())));
        assert_eq!(parse_art_url(&m).as_deref(), Some("https://i.scdn.co/image/abc"));
        m.insert("mpris:artUrl".to_string(), ov(Variant::Str("".into())));
        assert_eq!(parse_art_url(&m), None);
    }


    #[test]
    fn art_urls() {
        assert_eq!(art_path("file:///tmp/a%20b.png").unwrap().to_str(), Some("/tmp/a b.png"));
        assert_eq!(art_path("file://localhost/tmp/c.jpg").unwrap().to_str(), Some("/tmp/c.jpg"));
        assert_eq!(art_path("file:///tmp/100%.png").unwrap().to_str(), Some("/tmp/100%.png"));
        assert!(art_path("https://i.scdn.co/image/abc").is_none());
        assert!(art_path("http://x/y.png").is_none());
        assert!(art_path("file://host/x").is_none());
    }

    #[test]
    fn thumb_crops_and_averages() {
        // 4x2 image: left half black, right half white -> 2x2 centre crop = black|white, then 1px thumb = grey
        let mut img = vec![];
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                img.extend([v, v, v, 255]);
            }
        }
        let (n, t) = square_thumb(4, 2, &img, 8).unwrap();
        assert_eq!((n, t.len()), (2, 16)); // crop x=1..3: black col, white col
        assert_eq!(&t[..4], &[0, 0, 0, 255]);
        assert_eq!(&t[4..8], &[255, 255, 255, 255]);
        let (n, t) = square_thumb(4, 2, &img, 1).unwrap();
        assert_eq!((n, &t[..]), (1, &[127u8, 127, 127, 255][..]));
        assert!(square_thumb(0, 0, &[], 8).is_none());
    }

    #[test]
    fn remote_url_allow_deny() {
        for ok in ["https://i.scdn.co/image/ab67616d0000b273", "http://example.com/a.jpg", "https://a.b.example.org/x?y=1#z", "HTTPS://I.SCDN.CO/x".to_lowercase().as_str()] {
            assert!(remote_art_ok(ok), "{ok}");
        }
        for bad in [
            "file:///tmp/a.png", "ftp://example.com/a", "//example.com/a", "javascript:alert(1)", "httpx://example.com/a", "https:/example.com", "https://",
            "http://localhost/a", "http://localhost:8080/a", "http://foo.localhost/a", "http://127.0.0.1/a", "http://127.1/a", "http://2130706433/a", "http://0x7f.0.0.1/a",
            "http://10.0.0.1/a", "http://192.168.1.1/a", "http://169.254.169.254/latest", "http://172.16.0.1/a", "http://[::1]/a", "http://[::ffff:127.0.0.1]/a",
            "http://user:pw@example.com/a", "http://example.com:81/a", "http://router.local/a", "http://nas.lan/a", "http://printer/a", "http://exa mple.com/a",
            "http://example.com/a b", "http://example.com\\@127.0.0.1/", "http://-bad.example.com/", "http://a..b.com/", "",
        ] {
            assert!(!remote_art_ok(bad), "{bad}");
        }
    }

    #[test]
    fn cache_names() {
        let a = cache_name("https://i.scdn.co/image/abc");
        assert_eq!(a, cache_name("https://i.scdn.co/image/abc"));
        assert_ne!(a, cache_name("https://i.scdn.co/image/abd"));
        assert_eq!(a.len(), 20);
        assert!(a.ends_with(".img") && a[..16].bytes().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(cache_name(""), "cbf29ce484222325.img"); // FNV offset basis
    }

    #[test]
    fn magic_sniffing() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 13, 10]), Some(Img::Png));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some(Img::Jpeg));
        for no in [&b"<html>"[..], b"GIF89a", b"RIFF....WEBP", b"", &[0xff][..], &[0x89, b'P'][..]] {
            assert_eq!(sniff(no), None);
        }
    }

    #[test]
    fn eviction_drops_oldest_beyond_keep() {
        use std::time::{Duration, SystemTime};
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let f: Vec<_> = [("c", 30), ("a", 10), ("d", 40), ("b", 20)].iter().map(|(n, s)| (n.to_string(), t(*s))).collect();
        assert_eq!(evict_choice(&f, 2), vec!["b".to_string(), "a".to_string()]);
        assert!(evict_choice(&f, 4).is_empty() && evict_choice(&f, 50).is_empty());
        assert_eq!(evict_choice(&f, 0).len(), 4);
    }

    #[test]
    fn fetch_caches_once_rejects_junk_and_evicts() {
        // No network: `download` is a stand-in that writes bytes where curl would (a local substitute for the https fetch).
        use std::cell::Cell;
        let dir = std::env::temp_dir().join(format!("gk-art-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let calls = Cell::new(0);
        let dl = |_: &str, p: &std::path::Path| {
            calls.set(calls.get() + 1);
            std::fs::write(p, png).is_ok()
        };
        let u = "https://i.scdn.co/image/abc";
        assert_eq!(fetch_cached(u, &dir, &dl).unwrap(), png);
        assert_eq!(fetch_cached(u, &dir, &dl).unwrap(), png); // cache hit, no second download
        assert_eq!(calls.get(), 1);
        assert!(dir.join(cache_name(u)).exists());
        // junk body: not cached, no leftover tmp
        assert!(fetch_cached("https://example.com/x", &dir, |_: &str, p: &std::path::Path| std::fs::write(p, b"<html>").is_ok()).is_none());
        // disallowed URL never reaches the downloader
        assert!(fetch_cached("http://127.0.0.1/x", &dir, |_: &str, _: &std::path::Path| panic!("must not download")).is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        // cap at CACHE_KEEP
        for i in 0..CACHE_KEEP + 5 {
            fetch_cached(&format!("https://example.com/{i}"), &dir, &dl).unwrap();
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), CACHE_KEEP);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live check: art URL from the running player (or `GK_URL`), then `load_art`: `cargo test --offline live_art -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_art() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let bus = std::env::var("GK_BUS").unwrap_or("org.mpris.MediaPlayer2.spotify".into());
        let url = std::env::var("GK_URL").ok().or_else(|| rt.block_on(read_art_url(&bus))).expect("art url");
        eprintln!("url = {url}");
        let h = rt.block_on(load_art(url)).expect("decoded");
        eprintln!("handle = {h:?}");
        if let Ok(f) = std::env::var("GK_FILE") {
            assert!(decode_thumb(&std::fs::read(f).unwrap()).is_some()); // e.g. a cached cover
        }
    }
}
