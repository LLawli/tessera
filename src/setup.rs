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
