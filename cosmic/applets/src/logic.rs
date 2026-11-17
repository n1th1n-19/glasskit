// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure logic for the Status applet: state classification, parsing, tooltip text and the
//! SVG glyph bodies. No UI types in here so everything is unit-testable.

// ---- classification -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vol {
    Muted,
    Low,
    Mid,
    High,
}

pub fn vol_level(v: f32, muted: bool) -> Vol {
    if muted || v <= 0.0 {
        Vol::Muted
    } else if v <= 0.33 {
        Vol::Low
    } else if v <= 0.66 {
        Vol::Mid
    } else {
        Vol::High
    }
}

/// 1..=4 lit bars for a 0..=100 signal (the dot counts as bar one).
pub fn wifi_bars(signal: u8) -> u8 {
    match signal {
        0..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        _ => 4,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtState {
    Off,
    On,
    Connected,
}

pub fn bt_state(powered: bool, any_connected: bool) -> BtState {
    match (powered, any_connected) {
        (false, _) => BtState::Off,
        (true, false) => BtState::On,
        (true, true) => BtState::Connected,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattState {
    Unknown,
    Charging,
    /// on the charger and full
    Full,
    /// on the charger, neither charging nor draining (charge limit)
    Plugged,
    Critical,
    Low,
    Normal,
}

pub fn batt_state(pct: Option<f32>, status: Option<&str>) -> BattState {
    match (pct, status) {
        (_, Some("Charging")) => BattState::Charging,
        (None, _) => BattState::Unknown,
        (_, Some("Full")) => BattState::Full,
        (_, Some("Not charging")) => BattState::Plugged,
        (Some(p), _) if p <= 10.0 => BattState::Critical,
        (Some(p), _) if p <= 20.0 => BattState::Low,
        _ => BattState::Normal,
    }
}

/// Minutes until empty (discharging) or full (charging) from a power_supply `uevent`.
/// Energy and charge variants are both ratios, so no voltage is needed.
pub fn time_left_mins(uevent: &str) -> Option<u32> {
    let kv = |k: &str| uevent.lines().find_map(|l| l.strip_prefix("POWER_SUPPLY_")?.strip_prefix(k)?.strip_prefix('=')).and_then(|v| v.trim().parse::<f64>().ok());
    let pick = |a: &str, b: &str| kv(a).or_else(|| kv(b));
    let (now, full, rate) = (pick("ENERGY_NOW", "CHARGE_NOW")?, pick("ENERGY_FULL", "CHARGE_FULL"), pick("POWER_NOW", "CURRENT_NOW")?);
    if rate <= 0.0 {
        return None;
    }
    let status = uevent.lines().find_map(|l| l.strip_prefix("POWER_SUPPLY_STATUS="))?.trim();
    let hours = match status {
        "Discharging" => now / rate,
        "Charging" => (full? - now).max(0.0) / rate,
        _ => return None,
    };
    (hours.is_finite() && hours < 100.0).then(|| (hours * 60.0).round() as u32)
}

pub fn hours_text(mins: u32) -> String {
    if mins >= 60 { format!("{}h {:02}m", mins / 60, mins % 60) } else { format!("{mins}m") }
}

// ---- tooltip ------------------------------------------------------------------

pub fn vol_tip(o: Option<(f32, bool)>) -> String {
    match o {
        Some((v, false)) if v > 0.0 => format!("Sound: {:.0}%", v * 100.0),
        Some(_) => "Sound: muted".into(),
        None => "Sound: unavailable".into(),
    }
}

pub fn wifi_tip(enabled: Option<bool>, airplane: bool, ssid: Option<&str>) -> String {
    match (enabled, airplane, ssid) {
        (None, ..) => "Wi-Fi: unavailable".into(),
        (Some(false), ..) | (_, true, _) => "Wi-Fi: off".into(),
        (_, _, Some(s)) => format!("Wi-Fi: {s}"),
        _ => "Wi-Fi: not connected".into(),
    }
}

pub fn bt_tip(powered: Option<bool>, connected: &[&str]) -> String {
    match (powered, connected.is_empty()) {
        (None, _) => "Bluetooth: unavailable".into(),
        (Some(false), _) => "Bluetooth: off".into(),
        (_, true) => "Bluetooth: on".into(),
        (_, false) => format!("Bluetooth: {}", connected.join(", ")),
    }
}

pub fn batt_tip(pct: Option<f32>, status: Option<&str>, mins: Option<u32>) -> String {
    let Some(p) = pct else { return "Battery: unknown".into() };
    let mut s = format!("Battery {p:.0}%");
    match (batt_state(pct, status), mins) {
        (BattState::Charging, Some(m)) => s += &format!(" charging, {} until full", hours_text(m)),
        (BattState::Charging, None) => s += " charging",
        (BattState::Full, _) => s += ", full",
        (BattState::Plugged, _) => s += ", plugged in",
        (_, Some(m)) => s += &format!(", {} left", hours_text(m)),
        _ => {}
    }
    s
}

// ---- bluetoothctl -------------------------------------------------------------

pub fn valid_mac(s: &str) -> bool {
    s.len() == 17 && s.split(':').count() == 6 && s.split(':').all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Parse `bluetoothctl devices` (`Device AA:BB:CC:DD:EE:FF Name`) into (mac, name).
/// BlueZ names unnamed devices after their address (`AA-BB-...`); those are skipped.
pub fn parse_bt_devices(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ')?;
            let name = name.trim();
            (valid_mac(mac) && !name.is_empty() && name != mac.replace(':', "-")).then(|| (mac.to_string(), name.to_string()))
        })
        .collect()
}

/// BlueZ object path `/org/bluez/hci0/dev_3C_B0_ED_C1_DF_CC` is this MAC?
pub fn path_is_mac(path: &str, mac: &str) -> bool {
    path.rsplit('/').next().is_some_and(|d| d.eq_ignore_ascii_case(&format!("dev_{}", mac.replace(':', "_"))))
}

// ---- SVG glyphs (24x24 viewBox, explicit colour, filled / round-capped) ---------

fn doc(body: String) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{body}</svg>"#)
}

fn stroke(d: &str, c: &str, w: f32, op: f32) -> String {
    format!(r#"<path d="{d}" fill="none" stroke="{c}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round" opacity="{op}"/>"#)
}

pub fn volume(v: Vol, c: &str) -> String {
    let mut b = format!(r#"<path fill="{c}" d="M2.5 9.5h3L10.5 5.5v13l-5-4h-3a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1z"/>"#);
    let arcs = ["M13.5 9.6a3.4 3.4 0 0 1 0 4.8", "M16 7.2a6.6 6.6 0 0 1 0 9.6", "M18.5 4.8a9.8 9.8 0 0 1 0 14.4"];
    let n = match v {
        Vol::Muted => {
            b += &stroke("M14.5 9.5l6 5M20.5 9.5l-6 5", c, 2.5, 1.0);
            0
        }
        Vol::Low => 1,
        Vol::Mid => 2,
        Vol::High => 3,
    };
    for a in &arcs[..n] {
        b += &stroke(a, c, 2.5, 1.0);
    }
    doc(b)
}

/// `lit` 0..=4 bars (dot, then three arcs); unlit bars are dimmed; `slash` = radio off.
pub fn wifi(lit: u8, slash: bool, c: &str) -> String {
    let op = |n: u8| if lit >= n { 1.0 } else { 0.3 };
    let mut b = format!(r#"<circle cx="12" cy="19.4" r="1.9" fill="{c}" opacity="{}"/>"#, op(1));
    for (n, d) in [(2, "M8.46 15.86A5 5 0 0 1 15.54 15.86"), (3, "M5.64 13.04A9 9 0 0 1 18.36 13.04"), (4, "M2.81 10.21A13 13 0 0 1 21.19 10.21")] {
        b += &stroke(d, c, 2.5, op(n));
    }
    if slash {
        b += &stroke("M4.5 4l15 16", c, 2.5, 1.0);
    }
    doc(b)
}

pub fn bluetooth(s: BtState, c: &str) -> String {
    let rune = "M6.5 7.5L17 16.5L12 21V3L17 7.5L6.5 16.5";
    let mut b = stroke(rune, c, 2.5, if s == BtState::Off { 0.4 } else { 1.0 });
    match s {
        BtState::Off => b += &stroke("M4.5 4l15 16", c, 2.5, 1.0),
        BtState::Connected => b += &format!(r#"<circle cx="3.4" cy="12" r="1.6" fill="{c}"/><circle cx="20.6" cy="12" r="1.6" fill="{c}"/>"#),
        BtState::On => {}
    }
    doc(b)
}

pub fn battery(s: BattState, pct: f32, c: &str) -> String {
    let w = (14.2 * pct.clamp(0.0, 100.0) / 100.0).max(1.6);
    let mut b = format!(r#"<rect x="1.5" y="6.5" width="18" height="11" rx="2.8" fill="none" stroke="{c}" stroke-width="1.6"/><rect x="21" y="10" width="2" height="4" rx="1" fill="{c}"/>"#);
    let bolt = "M12.8 7.5L8.6 12.4h2.8l-.7 4.1 4.6-5.1h-2.9z";
    match s {
        BattState::Unknown => {
            b += &stroke("M10.3 10.4a1.8 1.8 0 1 1 2.5 1.7c-.6.3-.8.6-.8 1.3", c, 1.6, 1.0);
            b += &format!(r#"<circle cx="12" cy="15.5" r=".9" fill="{c}"/>"#);
        }
        _ => {
            b += &format!(r#"<rect x="3.4" y="8.4" width="{w}" height="7.2" rx="1.4" fill="{c}"/>"#);
            match s {
                BattState::Charging => b += &format!(r##"<path d="{bolt}" fill="#fff" stroke="#121419" stroke-width="1" stroke-linejoin="round" paint-order="stroke"/>"##),
                BattState::Plugged => b += &format!(r##"<path d="{bolt}" fill="none" stroke="#121419" stroke-width="2.4" stroke-linejoin="round"/><path d="{bolt}" fill="none" stroke="#fff" stroke-width="1" stroke-linejoin="round"/>"##),
                _ => {}
            }
        }
    }
    doc(b)
}

pub fn lock(c: &str) -> String {
    doc(format!(r#"<rect x="5" y="10.5" width="14" height="10.5" rx="2.5" fill="{c}"/>{}"#, stroke("M8.2 10.5V8a3.8 3.8 0 0 1 7.6 0v2.5", c, 2.0, 1.0)))
}

pub fn chevron(open: bool, c: &str) -> String {
    doc(stroke(if open { "M7 14.5l5-5 5 5" } else { "M7 9.5l5 5 5-5" }, c, 2.2, 1.0))
}

// ---- space applets ------------------------------------------------------------

pub const SPACE_MIN: u32 = 4;
pub const SPACE_MAX: u32 = 256;

/// Spacer length in logical px: `GLASSKIT_SPACE_PX` (when it parses) overrides the kind's `base`
/// for every spacer, clamped to 4..=256.
pub fn space_px(base: u32, env: Option<&str>) -> u32 {
    env.and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(base).clamp(SPACE_MIN, SPACE_MAX)
}

/// (width, height) of the spacer: `len` along the panel's main axis, `cross` (the applet height) across it.
pub fn space_dims(len: u32, cross: u32, horizontal: bool) -> (u32, u32) {
    if horizontal { (len, cross) } else { (cross, len) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember_core::control::network::{fields, parse_wifi};

    #[test]
    fn battery_classification() {
        use BattState::*;
        assert_eq!(batt_state(Some(5.0), Some("Charging")), Charging);
        assert_eq!(batt_state(Some(100.0), Some("Full")), Full);
        assert_eq!(batt_state(Some(80.0), Some("Not charging")), Plugged);
        assert_eq!(batt_state(Some(10.0), Some("Discharging")), Critical);
        assert_eq!(batt_state(Some(11.0), Some("Discharging")), Low);
        assert_eq!(batt_state(Some(20.0), Some("Discharging")), Low);
        assert_eq!(batt_state(Some(21.0), Some("Discharging")), Normal);
        assert_eq!(batt_state(Some(50.0), None), Normal);
        assert_eq!(batt_state(None, Some("Discharging")), Unknown);
        assert_eq!(batt_state(None, Some("Charging")), Charging);
    }

    #[test]
    fn levels() {
        assert_eq!(vol_level(0.5, true), Vol::Muted);
        assert_eq!(vol_level(0.0, false), Vol::Muted);
        assert_eq!([0.1, 0.5, 0.9].map(|v| vol_level(v, false)), [Vol::Low, Vol::Mid, Vol::High]);
        assert_eq!([0, 24, 25, 49, 50, 74, 75, 100].map(wifi_bars), [1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!(bt_state(false, true), BtState::Off);
        assert_eq!(bt_state(true, true), BtState::Connected);
    }

    #[test]
    fn time_left() {
        let dis = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_ENERGY_NOW=30000000\nPOWER_SUPPLY_ENERGY_FULL=60000000\nPOWER_SUPPLY_POWER_NOW=15000000\n";
        assert_eq!(time_left_mins(dis), Some(120));
        let chg = dis.replace("Discharging", "Charging");
        assert_eq!(time_left_mins(&chg), Some(120)); // (60-30)/15 h
        // charge/current flavour, held on the charger, and zero rate
        let cc = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_CHARGE_NOW=1000000\nPOWER_SUPPLY_CHARGE_FULL=4000000\nPOWER_SUPPLY_CURRENT_NOW=2000000\n";
        assert_eq!(time_left_mins(cc), Some(30));
        assert_eq!(time_left_mins(&dis.replace("Discharging", "Not charging")), None);
        assert_eq!(time_left_mins(&dis.replace("15000000", "0")), None);
        assert_eq!(hours_text(80), "1h 20m");
        assert_eq!(hours_text(45), "45m");
    }

    #[test]
    fn tooltips() {
        assert_eq!(batt_tip(Some(80.0), Some("Charging"), Some(80)), "Battery 80% charging, 1h 20m until full");
        assert_eq!(batt_tip(Some(55.0), Some("Discharging"), Some(125)), "Battery 55%, 2h 05m left");
        assert_eq!(batt_tip(Some(80.0), Some("Not charging"), None), "Battery 80%, plugged in");
        assert_eq!(wifi_tip(Some(true), false, Some("Home")), "Wi-Fi: Home");
        assert_eq!(wifi_tip(Some(true), false, None), "Wi-Fi: not connected");
        assert_eq!(wifi_tip(Some(true), true, Some("Home")), "Wi-Fi: off");
        assert_eq!(bt_tip(Some(true), &["CMF Buds", "Mouse"]), "Bluetooth: CMF Buds, Mouse");
        assert_eq!(vol_tip(Some((0.4, false))), "Sound: 40%");
        assert_eq!(vol_tip(Some((0.4, true))), "Sound: muted");
    }

    /// Real `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY dev wifi list` shapes: duplicate SSIDs
    /// across APs, hidden (empty) SSIDs, open network, escaped colon, WPA3-only.
    #[test]
    fn nmcli_wifi_list() {
        let out = " :Airtel_Optimus Prime:100:WPA2\n ::100:WPA2\n*:Airtel_Optimus Prime:78:WPA2\n :Cafe\\: Free:40:\n :Fancy:61:WPA3\n :Both:33:WPA2 WPA3\n";
        let known = vec!["Airtel_Optimus Prime".to_string()];
        let w = parse_wifi(out, &known);
        let rows: Vec<_> = w.iter().map(|n| (n.ssid.as_str(), n.signal, n.in_use, n.known, n.secure, n.wpa3)).collect();
        assert_eq!(
            rows,
            [
                ("Airtel_Optimus Prime", 100, true, true, true, false),
                ("Fancy", 61, false, false, true, true),
                ("Cafe: Free", 40, false, false, false, false),
                ("Both", 33, false, false, true, false),
            ]
        );
        assert_eq!(fields(r"a\:b:c"), ["a:b", "c"]);
    }

    #[test]
    fn bluetoothctl_devices() {
        let out = "Device C7:DC:89:08:16:20 Stone Uno\nDevice 41:6D:F6:EB:F2:64 Airdopes 219\nDevice 3C:B0:ED:C1:DF:CC CMF Buds\nDevice 41:42:3E:1B:C5:F6 ZEB-THUNDER MAX\nDevice AA:BB:CC:DD:EE:FF AA-BB-CC-DD-EE-FF\nController 70:08:94:E1:4C:9C pop-os\nDevice nonsense x\n[NEW] Device 11:22:33:44:55:66 Ghost\n";
        let d = parse_bt_devices(out);
        assert_eq!(d.iter().map(|d| d.1.as_str()).collect::<Vec<_>>(), ["Stone Uno", "Airdopes 219", "CMF Buds", "ZEB-THUNDER MAX"]);
        assert_eq!(d[2].0, "3C:B0:ED:C1:DF:CC");
        assert!(path_is_mac("/org/bluez/hci0/dev_3C_B0_ED_C1_DF_CC", "3C:B0:ED:C1:DF:CC"));
        assert!(!path_is_mac("/org/bluez/hci0/dev_3C_B0_ED_C1_DF_CC", "41:6D:F6:EB:F2:64"));
        assert!(valid_mac("3C:B0:ED:C1:DF:CC") && !valid_mac("--help") && !valid_mac("3C:B0:ED:C1:DF:CG"));
    }

    #[test]
    fn svgs_use_explicit_colour() {
        for s in [volume(Vol::High, "#abcdef"), wifi(3, false, "#abcdef"), bluetooth(BtState::Connected, "#abcdef"), battery(BattState::Charging, 50.0, "#abcdef"), lock("#abcdef"), chevron(true, "#abcdef")] {
            assert!(s.contains("#abcdef") && !s.contains("currentColor") && s.starts_with("<svg") && s.ends_with("</svg>"));
        }
    }

    #[test]
    fn space_sizes() {
        assert_eq!(space_px(16, None), 16);
        assert_eq!(space_px(16, Some("40")), 40);
        assert_eq!(space_px(16, Some(" 40 ")), 40);
        assert_eq!(space_px(16, Some("1")), 4);
        assert_eq!(space_px(16, Some("9999")), 256);
        assert_eq!(space_px(16, Some("junk")), 16);
        assert_eq!(space_px(16, Some("-3")), 16);
        assert_eq!(space_dims(16, 32, true), (16, 32));
        assert_eq!(space_dims(16, 32, false), (32, 16));
    }
}
