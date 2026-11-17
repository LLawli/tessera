//! One-time setup some drivers need on the host, done by the user with a
//! copied command (Tessera changes nothing outside its sandbox).
//!
//! A driver that talks to its token over USB itself, without pcscd (Feitian's
//! for the ePass2003 formatted by its Windows software), needs two things:
//!
//! - **USB**: Tessera sees the USB devices only when the user grants it
//!   (`flatpak override --user --device=usb`, or `--device=all` before
//!   Flatpak 1.15.11). Only the users of such a driver need it.
//! - **pcscd leaves the reader alone**: pcscd takes every CCID token; if it
//!   and the driver talk to the token at once, both hang and the token stops
//!   answering until it is unplugged (seen with the ePass2003). pcscd skips
//!   the readers named in PCSCLITE_FILTER_IGNORE_READER_NAMES, set in a
//!   drop-in for its service.
//!
//! Until both are done the driver is not served (supervise.rs): its server
//! would load the vendor's library into every listing and every app.

use std::path::Path;

use crate::drivers::Driver;
use crate::paths;

/// What a driver still needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    Usb,
    /// pcscd still lists this reader.
    PcscdIgnore(String),
}

/// One row of the window: something to do for a driver that is on, or
/// something to undo for one that is off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Do(Need),
    Undo(Need),
}

/// Tessera sees the USB devices (outside a Flatpak, always).
pub fn usb_visible() -> bool {
    !paths::in_flatpak() || Path::new("/dev/bus/usb").is_dir()
}

/// The Flatpak running Tessera, from /.flatpak-info (`flatpak-version=`).
fn flatpak_version() -> Option<(u32, u32, u32)> {
    let info = std::fs::read_to_string("/.flatpak-info").ok()?;
    parse_flatpak_version(&info)
}

fn parse_flatpak_version(info: &str) -> Option<(u32, u32, u32)> {
    let v = info
        .lines()
        .find_map(|l| l.trim().strip_prefix("flatpak-version="))?;
    let mut n = v.trim().split('.').map(|p| p.parse::<u32>().ok());
    Some((
        n.next()??,
        n.next().flatten().unwrap_or(0),
        n.next().flatten().unwrap_or(0),
    ))
}

/// The command that lets Tessera see the USB devices: `--device=usb` from
/// Flatpak 1.15.11 (only USB), `--device=all` before.
pub fn usb_command() -> String {
    let device = match flatpak_version() {
        Some(v) if v < (1, 15, 11) => "all",
        _ => "usb",
    };
    format!(
        "flatpak override --user --device={device} {}",
        paths::APP_ID
    )
}

/// The drop-in, one per reader name.
fn dropin(reader: &str) -> String {
    let slug: String = reader
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("/etc/systemd/system/pcscd.service.d/tessera-ignore-{slug}.conf")
}

/// The command that makes the system's pcscd ignore `reader`, then stops
/// it (socket activation starts it again, with the setting, at the next
/// use). Plain sh and fish alike.
pub fn pcscd_ignore_command(reader: &str) -> String {
    format!(
        "sudo mkdir -p /etc/systemd/system/pcscd.service.d && printf '[Service]\\nEnvironment=\"PCSCLITE_FILTER_IGNORE_READER_NAMES={reader}\"\\n' | sudo tee {} >/dev/null && sudo systemctl daemon-reload && sudo systemctl stop pcscd.service",
        dropin(reader)
    )
}

/// The command that undoes it.
pub fn pcscd_undo_command(reader: &str) -> String {
    format!(
        "sudo rm -f {} && sudo systemctl daemon-reload && sudo systemctl stop pcscd.service",
        dropin(reader)
    )
}

/// The command that takes the USB devices from Tessera again. Before
/// Flatpak 1.15.11 the grant was `--device=all`, and `--nodevice=all` would
/// take the graphics card too: there, Tessera's own overrides are reset.
pub fn usb_undo_command() -> String {
    match flatpak_version() {
        Some(v) if v < (1, 15, 11) => {
            format!("flatpak override --user --reset {}", paths::APP_ID)
        }
        _ => format!("flatpak override --user --nodevice=usb {}", paths::APP_ID),
    }
}

/// A USB device with this id ("vvvv:pppp") is plugged in, by the kernel's
/// list (which the sandbox sees without the USB devices themselves).
/// TESSERA_TEST_USB_IDS (ids split by `;`) stands in for it in tests.
pub fn usb_present(id: &str) -> bool {
    if let Ok(ids) = std::env::var("TESSERA_TEST_USB_IDS") {
        return ids.split(';').any(|i| i.eq_ignore_ascii_case(id));
    }
    let Ok(devices) = std::fs::read_dir("/sys/bus/usb/devices") else {
        return false;
    };
    devices.flatten().any(|d| {
        let read = |f: &str| std::fs::read_to_string(d.path().join(f)).unwrap_or_default();
        let found = format!("{}:{}", read("idVendor").trim(), read("idProduct").trim());
        found.eq_ignore_ascii_case(id)
    })
}

/// What is left of the one-time setup of `d`, to undo once it is off:
/// - the USB devices, still granted to Tessera;
/// - pcscd still skipping the reader, seen as the token plugged in
///   (`usb_id`) with no such reader listed. That hides the token from every
///   other driver: an ePass2003 formatted by OpenSC never reaches OpenSC.
pub fn leftover(d: &Driver, readers: &[String]) -> Vec<Need> {
    let mut out = vec![];
    if d.usb && paths::in_flatpak() && usb_visible() {
        out.push(Need::Usb);
    }
    if let (Some(r), Some(id)) = (&d.pcscd_ignore, &d.usb_id) {
        if usb_present(id) && !pcscd_lists(r, readers) {
            out.push(Need::PcscdIgnore(r.clone()));
        }
    }
    out
}

/// pcscd lists a reader whose name contains `filter` (pcscd matches its
/// ignore list the same way, by substring).
pub fn pcscd_lists(filter: &str, readers: &[String]) -> bool {
    readers.iter().any(|r| r.contains(filter))
}

/// What `d` still needs, given the readers pcscd lists now. A reader pcscd
/// does not list is taken as ignored: with the token unplugged there is
/// nothing to tell, and nothing to collide with either.
pub fn unmet(d: &Driver, readers: &[String]) -> Vec<Need> {
    let mut out = vec![];
    if d.usb && !usb_visible() {
        out.push(Need::Usb);
    }
    if let Some(r) = &d.pcscd_ignore {
        if pcscd_lists(r, readers) {
            out.push(Need::PcscdIgnore(r.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_version_from_the_info_file() {
        let info = "[Application]\nname=x\n\n[Instance]\nflatpak-version=1.16.1\n";
        assert_eq!(parse_flatpak_version(info), Some((1, 16, 1)));
        assert_eq!(
            parse_flatpak_version("flatpak-version=1.14\n"),
            Some((1, 14, 0))
        );
        assert_eq!(parse_flatpak_version("[Instance]\n"), None);
        assert!((1, 14, 6) < (1, 15, 11) && (1, 16, 0) >= (1, 15, 11));
    }

    #[test]
    fn the_commands_name_the_reader_and_undo_each_other() {
        let on = pcscd_ignore_command("Feitian ePass2003");
        let off = pcscd_undo_command("Feitian ePass2003");
        let file = "/etc/systemd/system/pcscd.service.d/tessera-ignore-feitian-epass2003.conf";
        assert!(on.contains(file) && off.contains(file));
        assert!(on.contains("PCSCLITE_FILTER_IGNORE_READER_NAMES=Feitian ePass2003"));
        assert!(usb_command().ends_with(paths::APP_ID));
    }

    #[test]
    fn the_reader_counts_while_pcscd_lists_it() {
        let readers = vec![
            "Feitian ePass2003 00 00".to_string(),
            "GD Burti 01 00".into(),
        ];
        assert!(pcscd_lists("Feitian ePass2003", &readers));
        assert!(!pcscd_lists("Feitian ePass2003", &readers[1..]));
    }

    /// Off, a driver's setup is left over while the token is plugged in
    /// and pcscd does not list its reader.
    #[test]
    fn leftover_is_the_hidden_reader_of_a_plugged_token() {
        let d = Driver {
            id: "epass2003".into(),
            name: "x".into(),
            vendor: String::new(),
            description: String::new(),
            module: "/x".into(),
            tools: vec![],
            create_dirs: vec![],
            library_path: vec![],
            env: Default::default(),
            when_on: Default::default(),
            usb: false,
            pcscd_ignore: Some("Feitian ePass2003".into()),
            usb_id: Some("096e:0807".into()),
            patient: false,
        };
        let listed = vec!["Feitian ePass2003 00 00".to_string()];
        std::env::set_var("TESSERA_TEST_USB_IDS", "046d:0825;096E:0807");
        assert_eq!(
            leftover(&d, &[]),
            vec![Need::PcscdIgnore("Feitian ePass2003".into())]
        );
        assert!(leftover(&d, &listed).is_empty());
        std::env::set_var("TESSERA_TEST_USB_IDS", "046d:0825");
        assert!(leftover(&d, &[]).is_empty());
        std::env::remove_var("TESSERA_TEST_USB_IDS");
        assert!(usb_undo_command().ends_with(paths::APP_ID));
    }

    /// The printf in the command writes the drop-in systemd expects.
    #[test]
    fn the_dropin_is_what_systemd_reads() {
        let cmd = pcscd_ignore_command("Feitian ePass2003");
        let start = cmd.find("printf '").unwrap() + 8;
        let fmt = &cmd[start..start + cmd[start..].find('\'').unwrap()];
        let out = std::process::Command::new("printf")
            .arg(fmt)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            "[Service]\nEnvironment=\"PCSCLITE_FILTER_IGNORE_READER_NAMES=Feitian ePass2003\"\n"
        );
    }
}
