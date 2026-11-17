// SPDX-License-Identifier: GPL-3.0-or-later
//! .desktop discovery, ranking, Exec handling, recents. Pure where it can be (tested below).
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub generic: String,
    pub keywords: String,
    pub icon: String,
    pub exec: String,
    pub terminal: bool,
}

fn truthy(v: &str) -> bool {
    v.trim().eq_ignore_ascii_case("true")
}

/// Parse one .desktop file's [Desktop Entry] group. None = hidden / not an application / not for `desktop`.
pub fn parse(id: &str, text: &str, desktop: &[String]) -> Option<App> {
    let mut a = App { id: id.into(), ..Default::default() };
    let (mut in_entry, mut kind_ok) = (false, true);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| in_entry) else { continue };
        let v = v.trim();
        match k.trim() {
            "Type" => kind_ok = v == "Application",
            "Name" => a.name = v.into(),
            "GenericName" => a.generic = v.into(),
            "Keywords" => a.keywords = v.replace(';', " "),
            "Icon" => a.icon = v.into(),
            "Exec" => a.exec = v.into(),
            "Terminal" => a.terminal = truthy(v),
            "NoDisplay" | "Hidden" if truthy(v) => return None,
            "OnlyShowIn" if !v.split(';').any(|d| desktop.iter().any(|x| x == d)) => return None,
            "NotShowIn" if v.split(';').any(|d| desktop.iter().any(|x| x == d)) => return None,
            _ => {}
        }
    }
    (kind_ok && !a.name.is_empty() && !a.exec.is_empty()).then_some(a)
}

fn walk(root: &Path, dir: &Path, desktop: &[String], seen: &mut HashSet<String>, out: &mut Vec<App>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, desktop, seen, out);
        } else if p.extension().is_some_and(|x| x == "desktop") {
            let id = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('/', "-");
            // earlier dirs (user) win over later (system), like the XDG spec
            if seen.insert(id.clone()) {
                if let Some(a) = std::fs::read_to_string(&p).ok().and_then(|t| parse(&id, &t, desktop)) {
                    out.push(a);
                }
            }
        }
    }
}

pub fn load() -> Vec<App> {
    let home = std::env::var("HOME").unwrap_or_default();
    let desktop: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').map(String::from).collect();
    let mut dirs: Vec<PathBuf> = vec![std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| format!("{home}/.local/share").into())];
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.push("/var/lib/flatpak/exports/share".into());
    dirs.push(format!("{home}/.local/share/flatpak/exports/share").into());
    let (mut seen, mut out) = (HashSet::new(), Vec::new());
    for d in dirs {
        let d = d.join("applications");
        walk(&d, &d, &desktop, &mut seen, &mut out);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Score of `a` for query `q` (already lowercase, non-empty); lower is better.
pub fn score(a: &App, q: &str) -> Option<u8> {
    let name = a.name.to_lowercase();
    if name.starts_with(q) {
        return Some(0);
    }
    if name.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q)) {
        return Some(1);
    }
    if name.contains(q) {
        return Some(2);
    }
    if a.generic.to_lowercase().contains(q) || a.keywords.to_lowercase().contains(q) {
        return Some(3);
    }
    // fuzzy: query chars appear in order in the name
    let mut it = name.chars();
    q.chars().filter(|c| !c.is_whitespace()).all(|c| it.any(|n| n == c)).then_some(4)
}

/// Indices into `apps`, best first. Empty query: recents (most recent first), then alphabetical (apps is pre-sorted).
pub fn rank(apps: &[App], q: &str, recent: &[String]) -> Vec<usize> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        let mut v: Vec<usize> = recent.iter().filter_map(|r| apps.iter().position(|a| &a.id == r)).collect();
        let first: HashSet<usize> = v.iter().copied().collect();
        v.extend((0..apps.len()).filter(|i| !first.contains(i)));
        return v;
    }
    let mut v: Vec<(u8, usize)> = apps.iter().enumerate().filter_map(|(i, a)| score(a, &q).map(|s| (s, i))).collect();
    v.sort(); // ties keep alphabetical order via index
    v.into_iter().map(|(_, i)| i).collect()
}

/// Drop Exec field codes (%f %F %u %U %i %c %k and the deprecated ones); `%%` is a literal percent.
pub fn strip_exec(exec: &str) -> String {
    let mut out = String::new();
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if let Some(n) = it.next() {
            if n == '%' {
                out.push('%');
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Run the app detached (own session) so it outlives the launcher.
pub fn launch(a: &App) {
    let cmd = strip_exec(&a.exec);
    let cmd = if a.terminal { format!("cosmic-term -e {cmd}") } else { cmd };
    let _ = std::process::Command::new("setsid")
        .args(["-f", "sh", "-c", &format!("exec {cmd}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| c.wait());
}

fn state_path(name: &str) -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| format!("{}/.local/state", std::env::var("HOME").unwrap_or_default()));
    PathBuf::from(base).join("glasskit-launcher").join(name)
}

fn recent_path() -> PathBuf {
    state_path("recent")
}

fn write_lines(p: PathBuf, v: &[String]) {
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, v.join("\n"));
}

pub fn recent_load() -> Vec<String> {
    std::fs::read_to_string(recent_path()).map(|s| s.lines().map(String::from).collect()).unwrap_or_default()
}

pub fn recent_push(id: &str) {
    let mut v = recent_load();
    v.retain(|x| x != id);
    v.insert(0, id.into());
    v.truncate(40);
    write_lines(recent_path(), &v);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    All,
    Favs,
}

/// Pin if absent (appended, so order is pin order), unpin if present.
pub fn toggle_pin(pins: &mut Vec<String>, id: &str) {
    match pins.iter().position(|p| p == id) {
        Some(i) => {
            pins.remove(i);
        }
        None => pins.push(id.into()),
    }
}

/// Every double-quoted string in a RON list like `["a", "b"]` (COSMIC dock favourites). No escape handling: desktop ids have none.
pub fn parse_favorites(ron: &str) -> Vec<String> {
    ron.split('"').skip(1).step_by(2).map(String::from).collect()
}

/// Initial pins: dock favourites that exist as apps; if none, the 12 most recent apps that exist.
pub fn seed(apps: &[App], fav_ron: Option<&str>, recent: &[String]) -> Vec<String> {
    let known = |id: &String| apps.iter().any(|a| &a.id == id);
    let mut out: Vec<String> = Vec::new();
    for id in fav_ron.map(parse_favorites).unwrap_or_default() {
        // dock favourites omit the ".desktop" suffix that our ids carry
        let id = if id.ends_with(".desktop") { id } else { format!("{id}.desktop") };
        if known(&id) && !out.contains(&id) {
            out.push(id);
        }
    }
    if out.is_empty() {
        out = recent.iter().filter(|r| known(r)).take(12).cloned().collect();
    }
    out
}

/// None = pin file missing (first run).
pub fn pins_load() -> Option<Vec<String>> {
    std::fs::read_to_string(state_path("pinned")).ok().map(|s| s.lines().filter(|l| !l.is_empty()).map(String::from).collect())
}

pub fn pins_save(pins: &[String]) {
    write_lines(state_path("pinned"), pins);
}

/// Load pins, seeding (and saving) them from the COSMIC dock favourites on first run.
pub fn pins_init(apps: &[App], recent: &[String]) -> Vec<String> {
    if let Some(p) = pins_load() {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let fav = std::fs::read_to_string(format!("{home}/.config/cosmic/com.system76.CosmicAppList/v1/favorites")).ok();
    let p = seed(apps, fav.as_deref(), recent);
    if !p.is_empty() {
        pins_save(&p); // an empty seed is not saved, so a later run can seed again
    }
    p
}

/// Indices into `apps` for the active tab. Favourites with no query: pin order; with a query: ranked, pinned only.
pub fn rank_tab(apps: &[App], q: &str, recent: &[String], pins: &[String], tab: Tab) -> Vec<usize> {
    match tab {
        Tab::All => rank(apps, q, recent),
        Tab::Favs if q.trim().is_empty() => pins.iter().filter_map(|p| apps.iter().position(|a| &a.id == p)).collect(),
        Tab::Favs => rank(apps, q, recent).into_iter().filter(|&i| pins.contains(&apps[i].id)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, generic: &str, kw: &str) -> App {
        App { id: id.into(), name: name.into(), generic: generic.into(), keywords: kw.into(), exec: "x".into(), ..Default::default() }
    }

    #[test]
    fn strips_field_codes() {
        assert_eq!(strip_exec("firefox %u"), "firefox");
        assert_eq!(strip_exec("app --icon %i --name %c %k %F %f %U"), "app --icon --name");
        assert_eq!(strip_exec("sh -c 'echo 100%%'"), "sh -c 'echo 100%'");
    }

    #[test]
    fn ranks() {
        let apps = vec![app("a", "Files", "File Manager", ""), app("b", "Firefox", "Web Browser", "internet"), app("c", "Text Editor", "", "notes"), app("d", "Terminal", "", "")];
        assert_eq!(rank(&apps, "FI", &[]), vec![0, 1]); // prefix, alphabetical
        assert_eq!(rank(&apps, "edit", &[]), vec![2]);
        assert_eq!(rank(&apps, "browser", &[]), vec![1]); // generic name
        assert_eq!(rank(&apps, "notes", &[]), vec![2]); // keywords
        assert_eq!(rank(&apps, "tmnl", &[]), vec![3]); // fuzzy
        assert!(rank(&apps, "zzz", &[]).is_empty());
        assert_eq!(rank(&apps, "", &["d".into(), "b".into(), "gone".into()]), vec![3, 1, 0, 2]); // recents first
    }

    #[test]
    fn pin_toggle_keeps_order() {
        let mut p: Vec<String> = vec![];
        for id in ["b", "a", "c"] {
            toggle_pin(&mut p, id);
        }
        assert_eq!(p, ["b", "a", "c"]);
        toggle_pin(&mut p, "a");
        assert_eq!(p, ["b", "c"]);
        toggle_pin(&mut p, "a");
        assert_eq!(p, ["b", "c", "a"]);
    }

    #[test]
    fn seeds() {
        let apps = vec![app("a.desktop", "A", "", ""), app("b.desktop", "B", "", ""), app("c.desktop", "C", "", "")];
        let ron = "[\n    \"c\",\n    \"gone\",\n    \"a.desktop\",\n    \"c\",\n]";
        assert_eq!(parse_favorites(ron), ["c", "gone", "a.desktop", "c"]);
        assert_eq!(parse_favorites("[]"), Vec::<String>::new());
        assert_eq!(seed(&apps, Some(ron), &["b.desktop".into()]), ["c.desktop", "a.desktop"]); // existing only, deduped, dock order
        let recent: Vec<String> = ["b.desktop", "x", "a.desktop"].iter().map(|s| s.to_string()).collect();
        assert_eq!(seed(&apps, None, &recent), ["b.desktop", "a.desktop"]); // no file: recents
        assert_eq!(seed(&apps, Some("[]"), &recent), ["b.desktop", "a.desktop"]); // empty file: recents
        let many: Vec<App> = (0..20).map(|i| app(&format!("i{i}"), &format!("N{i}"), "", "")).collect();
        let rec: Vec<String> = (0..20).map(|i| format!("i{i}")).collect();
        assert_eq!(seed(&many, None, &rec).len(), 12);
    }

    #[test]
    fn tab_filtering() {
        let apps = vec![app("a", "Files", "", ""), app("b", "Firefox", "", ""), app("c", "Terminal", "", "")];
        let pins: Vec<String> = vec!["c".into(), "b".into(), "gone".into()];
        assert_eq!(rank_tab(&apps, "", &[], &pins, Tab::All), vec![0, 1, 2]);
        assert_eq!(rank_tab(&apps, "", &[], &pins, Tab::Favs), vec![2, 1]); // pin order
        assert_eq!(rank_tab(&apps, "fi", &[], &pins, Tab::Favs), vec![1]); // only pinned are searched
        assert_eq!(rank_tab(&apps, "fi", &[], &pins, Tab::All), vec![0, 1]);
        assert!(rank_tab(&apps, "", &[], &[], Tab::Favs).is_empty());
    }

    #[test]
    fn parses_visibility() {
        let de = vec!["COSMIC".to_string()];
        let t = "[Desktop Entry]\nType=Application\nName=X\nExec=x %u\nOnlyShowIn=GNOME;\n";
        assert!(parse("x", t, &de).is_none());
        assert!(parse("x", &t.replace("GNOME", "COSMIC"), &de).is_some());
        assert!(parse("x", "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n", &de).is_none());
        // keys in other groups are ignored
        assert!(parse("x", "[Desktop Entry]\nType=Application\nName=X\nExec=x\n[Desktop Action a]\nNoDisplay=true\n", &de).is_some());
    }
}
