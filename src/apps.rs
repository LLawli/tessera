//! The host side and the client apps.
//!
//! Host: one file per driver in ~/.config/pkcs11/modules,
//!
//!     remote: unix:path=/run/user/<uid>/tessera/<driver>
//!
//! makes the host's p11-kit (its proxy, and its server behind
//! `p11-kit-server.socket`, or on Debian a plain user service) serve Tessera's tokens next to the host's own
//! modules. Native apps that use p11-kit-proxy.so see them directly.
//!
//! Sandboxed apps: `flatpak override --user
//! --filesystem=xdg-run/p11-kit/pkcs11 <app>` puts the host's server socket
//! where p11-kit-client.so looks for it, in place of Flatpak's trust-store
//! server (the host server serves the host's trust store too).

use anyhow::{Context, Result};
use std::fs;

use crate::drivers::Driver;
use crate::paths;

/// How an app is set up once it may reach the host's p11-kit server.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    /// Register p11-kit-client.so in every profile listed by the
    /// profiles.ini in these folders of the app's own (~/.var/app/<id>/…):
    /// Firefox and Thunderbird.
    Profiles(&'static [&'static str]),
    /// Signs through poppler: an empty NSS database of its own, in the
    /// first place poppler looks (NSS's roots module, p11-kit in the
    /// runtime, already reaches the server).
    Poppler,
    /// Register p11-kit-client.so in the browser's ~/.pki/nssdb.
    Chromium,
    /// An empty NSS database of its own, named in its settings (CertDir):
    /// see libreoffice.rs.
    LibreOffice,
    /// p11-kit-client.so and the slot of Tessera's tokens in its pkcs11.cfg:
    /// see jsignpdf.rs.
    JSignPdf,
    /// p11-kit-client.so as the PKCS#11 provider in its settings file: see
    /// masterpdf.rs.
    MasterPdf,
    /// By hand in the app's settings: the row shows where, and the module's
    /// path.
    ByHand,
    /// Nothing: the app's package already does it (PJeOffice Pro).
    None,
}

pub struct ClientApp {
    pub id: &'static str,
    pub name: &'static str,
    /// A generic icon, for when the app's own is not available.
    pub icon: &'static str,
    /// Tessera's copy of the app's icon (data/app-icons), when its license
    /// allows one.
    pub bundled_icon: Option<&'static str>,
    pub setup: Setup,
    /// Where, in the app, the module can be loaded by hand (shown with
    /// p11-kit-client.so's path).
    pub by_hand: Option<&'static str>,
}

pub const FIREFOX: ClientApp = ClientApp {
    id: "org.mozilla.firefox",
    name: "Firefox",
    icon: "web-browser-symbolic",
    bundled_icon: None,
    // Firefox 147+ keeps its profiles under .config, older releases under
    // .mozilla.
    setup: Setup::Profiles(&["config/mozilla/firefox", ".mozilla/firefox"]),
    by_hand: Some("Or by hand in Firefox: Settings → Privacy & Security → Security Devices → Load, module file:"),
};
/// Thunderbird's Flatpak, built by Mozilla. Its ESR has an ID of its own,
/// and the old ID (org.mozilla.Thunderbird) stopped being updated in 2026;
/// those two show only when installed. Mozilla's builds carry their own NSS,
/// which does not load the runtime's p11-kit by itself: like Firefox, the
/// module goes into every profile.
pub const THUNDERBIRD: ClientApp = ClientApp {
    id: "org.mozilla.thunderbird",
    name: "Thunderbird",
    icon: "mail-send-symbolic",
    bundled_icon: None,
    setup: Setup::Profiles(THUNDERBIRD_ROOTS),
    by_hand: Some(THUNDERBIRD_BY_HAND),
};
pub const THUNDERBIRD_ESR: ClientApp = ClientApp {
    id: "org.mozilla.thunderbird_esr",
    name: "Thunderbird ESR",
    ..THUNDERBIRD
};
pub const THUNDERBIRD_OLD: ClientApp = ClientApp {
    id: "org.mozilla.Thunderbird",
    name: "Thunderbird",
    ..THUNDERBIRD
};
const THUNDERBIRD_BY_HAND: &str =
    "Or by hand in Thunderbird: Settings → Privacy & Security → Security Devices → Load, module file:";
/// Evolution's Flatpak reads the real ~/.pki/nssdb when there is one, else
/// its own data/pki/nssdb (evolution-data-server 3.62). When it starts its
/// S/MIME certificates it adds NSS's root-certificates module, which in the
/// runtime is p11-kit's trust module: as for Chromium, step 1 is enough.
pub const EVOLUTION: ClientApp = ClientApp {
    id: "org.gnome.Evolution",
    name: "Evolution",
    icon: "mail-send-symbolic",
    bundled_icon: None,
    setup: Setup::Chromium,
    by_hand: None,
};
pub const CHROMIUM: ClientApp = ClientApp {
    id: "org.chromium.Chromium",
    name: "Chromium",
    icon: "web-browser-symbolic",
    bundled_icon: Some("tessera-app-chromium"),
    setup: Setup::Chromium,
    by_hand: None,
};
pub const CHROME: ClientApp = ClientApp {
    id: "com.google.Chrome",
    name: "Google Chrome",
    icon: "web-browser-symbolic",
    bundled_icon: None,
    setup: Setup::Chromium,
    by_hand: None,
};
pub const EDGE: ClientApp = ClientApp {
    id: "com.microsoft.Edge",
    name: "Microsoft Edge",
    icon: "web-browser-symbolic",
    bundled_icon: None,
    setup: Setup::Chromium,
    by_hand: None,
};
pub const BRAVE: ClientApp = ClientApp {
    id: "com.brave.Browser",
    name: "Brave",
    icon: "web-browser-symbolic",
    bundled_icon: None,
    setup: Setup::Chromium,
    by_hand: None,
};
pub const PAPERS: ClientApp = ClientApp {
    id: "org.gnome.Papers",
    name: "Papers",
    icon: "x-office-document-symbolic",
    bundled_icon: Some("tessera-app-papers"),
    setup: Setup::Poppler,
    by_hand: None,
};
/// Okular signs through poppler too (KDE runtime, same freedesktop base).
pub const OKULAR: ClientApp = ClientApp {
    id: "org.kde.okular",
    name: "Okular",
    icon: "x-office-document-symbolic",
    bundled_icon: Some("tessera-app-okular"),
    setup: Setup::Poppler,
    by_hand: None,
};
pub const LIBREOFFICE: ClientApp = ClientApp {
    id: crate::libreoffice::FLATPAK_ID,
    name: "LibreOffice",
    icon: "x-office-document-symbolic",
    bundled_icon: None,
    setup: Setup::LibreOffice,
    by_hand: None,
};
pub const JSIGNPDF: ClientApp = ClientApp {
    id: crate::jsignpdf::FLATPAK_ID,
    name: "JSignPdf",
    icon: "document-edit-symbolic",
    bundled_icon: None,
    setup: Setup::JSignPdf,
    by_hand: None,
};
/// Master PDF Editor (Flathub, Qt 6 build) loads one PKCS#11 library,
/// chosen in Tools → Settings → Certificates → "PKCS#11 provider": see
/// masterpdf.rs. The row also shows where, for doing it by hand.
pub const MASTERPDF: ClientApp = ClientApp {
    id: crate::masterpdf::FLATPAK_ID,
    name: "Master PDF Editor",
    icon: "x-office-document-symbolic",
    bundled_icon: None,
    setup: Setup::MasterPdf,
    by_hand: Some(
        "Or by hand in Master PDF Editor: Tools → Settings → Certificates → PKCS#11 provider, file:",
    ),
};
pub const PJEOFFICE: ClientApp = ClientApp {
    id: "br.jus.cnj.PJeOfficePro",
    name: "PJeOffice Pro",
    icon: "document-edit-symbolic",
    bundled_icon: None,
    setup: Setup::None,
    by_hand: None,
};
/// Assinador SERPRO (FlatPark): its package already has the p11-kit socket
/// and its own bridge to p11-kit-client.so (which hides the trust slots).
pub const ASSINADOR: ClientApp = ClientApp {
    id: "br.gov.serpro.desktop.assinador",
    name: "Assinador SERPRO",
    icon: "document-edit-symbolic",
    bundled_icon: None,
    setup: Setup::None,
    by_hand: None,
};

/// Where Thunderbird's Flatpak keeps its profiles, in its own folder
/// (--persist=.thunderbird; newer releases may use the XDG one).
const THUNDERBIRD_ROOTS: &[&str] = &[".thunderbird", "config/thunderbird"];

/// The apps with steps, shown in the window, in order.
pub const APPS: [&ClientApp; 14] = [
    &FIREFOX,
    &CHROMIUM,
    &CHROME,
    &EDGE,
    &BRAVE,
    &THUNDERBIRD,
    &THUNDERBIRD_ESR,
    &THUNDERBIRD_OLD,
    &EVOLUTION,
    &PAPERS,
    &OKULAR,
    &LIBREOFFICE,
    &JSIGNPDF,
    &MASTERPDF,
];

/// Is the app's row shown? Thunderbird's ESR and old ID only when they are
/// installed.
pub fn shown(app: &ClientApp) -> bool {
    ![THUNDERBIRD_ESR.id, THUNDERBIRD_OLD.id].contains(&app.id) || paths::flatpak_installed(app.id)
}

/// The apps whose package already reaches the system's p11-kit server.
pub const NO_STEPS: [&ClientApp; 2] = [&PJEOFFICE, &ASSINADOR];

/// The profiles of an app that keeps Mozilla-style profiles in its own
/// folder.
pub fn app_profiles(app: &ClientApp) -> Vec<std::path::PathBuf> {
    match app.setup {
        Setup::Profiles(roots) => {
            let base = paths::home().join(".var/app").join(app.id);
            let roots: Vec<_> = roots.iter().map(|r| base.join(r)).collect();
            paths::profiles_in(&roots)
        }
        _ => vec![],
    }
}

/// The one-time host command that starts the host's p11-kit server.
const SOCKET_COMMAND: &str = "systemctl --user enable --now p11-kit-server.socket";

/// Debian and its derivatives (Ubuntu, Mint) ship the server (`p11-kit
/// server`) but no systemd units for it, so the command writes user units,
/// upstream's own (p11-kit/p11-kit-server.{socket,service}): a socket unit at
/// the path the other distributions use, and the service it starts. p11-kit
/// takes the socket from systemd since 0.25.6, without libsystemd (the
/// Debian family builds it without). Each line goes through printf, so that
/// systemd's `%t` (the runtime folder) reaches the files as it is.
///
/// Why a socket unit: Flatpak binds the socket file itself into each sandbox
/// when an app starts, so a server that makes its own socket left every app
/// opened before its restart with a dead one; the socket unit keeps the same
/// socket across the server's restarts. No `-n` on the command line, unlike
/// upstream's unit: p11-kit removes the file of that name when it stops,
/// even a socket systemd made, which left the socket unit listening on a
/// file no longer there after the first restart (seen on Ubuntu 24.04 and
/// 26.04). Without it, the name is one of its own (pkcs11-<pid>), never made
/// when systemd hands it the socket.
///
/// The server running until now (a plain service, from LEGACY_COMMAND or an
/// older Tessera) is stopped first, and the socket made anew once: Tessera,
/// opened before, still holds the old one, so the command ends by opening it
/// again (nothing happens if it is not installed yet). The apps opened
/// before need reopening too, as after any first set-up.
fn socket_units_command(description: &str, env: Option<&str>, p11kit: &str) -> String {
    let lines = |ls: &[String]| -> String {
        ls.iter()
            .map(|l| format!("'{}'", l.replace('\'', r"'\''")))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let socket = lines(&[
        "[Unit]".into(),
        format!("Description={description}"),
        "[Socket]".into(),
        "Priority=6".into(),
        "Backlog=5".into(),
        "ListenStream=%t/p11-kit/pkcs11".into(),
        "SocketMode=0600".into(),
        "[Install]".into(),
        "WantedBy=sockets.target".into(),
    ]);
    let mut service = vec![
        "[Unit]".to_string(),
        format!("Description={description}"),
        "Documentation=man:p11-kit(8)".into(),
        "Requires=p11-kit-server.socket".into(),
        "[Service]".into(),
        "Type=simple".into(),
    ];
    if let Some(env) = env {
        service.push(format!("Environment={env}"));
    }
    service.extend([
        format!("ExecStart={p11kit} server -f pkcs11:"),
        "Restart=on-failure".into(),
        "[Install]".into(),
        "Also=p11-kit-server.socket".into(),
        "WantedBy=default.target".into(),
    ]);
    let service = lines(&service);
    let app = paths::APP_ID;
    format!(
        "mkdir -p ~/.config/systemd/user && cd ~/.config/systemd/user && \
         printf '%s\\n' {socket} > p11-kit-server.socket && \
         printf '%s\\n' {service} > p11-kit-server.service && \
         systemctl --user daemon-reload && \
         {{ systemctl --user stop p11-kit-server.service; true; }} && \
         systemctl --user enable p11-kit-server.service && \
         systemctl --user start p11-kit-server.socket p11-kit-server.service && \
         {{ flatpak kill {app} 2>/dev/null; (setsid flatpak run {app} >/dev/null 2>&1 &); }}"
    )
}

/// The Debian family with p11-kit 0.25.6 or newer (Ubuntu 26.04, Debian
/// testing): the system's own server, through socket_units_command.
fn debian_command() -> String {
    socket_units_command("p11-kit server", None, "/usr/bin/p11-kit")
}

/// The Debian releases whose p11-kit is 0.25.5 or older and can run
/// Tessera's (native.rs, HostP11kit::Fixable): the same units, running
/// Tessera's p11-kit from Flatpak's deployment of Tessera on the host (`dir`,
/// see tessera_p11kit_dir) in place of the system's. It serves the same
/// configuration (the system's modules, its trust and Tessera's drivers) at
/// the same path, and speaks with the Flatpak apps' newer p11-kit as well as
/// with older ones. P11_KIT_PRIVATEDIR is where the command finds its two
/// helpers.
fn hub_command(dir: &str) -> String {
    socket_units_command(
        "p11-kit server (Tessera's p11-kit)",
        Some(&format!("P11_KIT_PRIVATEDIR={dir}")),
        &format!("{dir}/p11-kit"),
    )
}

/// A p11-kit of 0.25.5 or older without Tessera's at hand (Debian 12, Ubuntu
/// 22.04, or Tessera run outside Flatpak): it takes no socket from systemd,
/// so a plain user service, started at login, that makes the socket itself.
/// Signing does not work there anyway; the tokens and certificates do.
const LEGACY_COMMAND: &str = concat!(
    "mkdir -p ~/.config/systemd/user && printf '%s\\n' ",
    "'[Unit]' 'Description=p11-kit server' 'Documentation=man:p11-kit(8)' ",
    "'[Service]' 'ExecStartPre=/bin/mkdir -p -m 700 %t/p11-kit' ",
    "'ExecStart=/usr/bin/p11-kit server -f -n %t/p11-kit/pkcs11 pkcs11:' ",
    "'Restart=on-failure' '[Install]' 'WantedBy=default.target' ",
    "> ~/.config/systemd/user/p11-kit-server.service && ",
    "systemctl --user daemon-reload && ",
    "systemctl --user enable --now p11-kit-server.service"
);

/// Where Tessera's own p11-kit (the manifest's p11-kit module) is on the
/// host. Inside the sandbox it is lib/tessera/p11-kit next to the running
/// tessera's bin/: /app/lib/tessera/p11-kit in Tessera's own build,
/// /app/extra/tessera/lib/tessera/p11-kit in FlatPark's package, which
/// unpacks that build under /app/extra/tessera. On the host, /app is the
/// deployment's files/ folder, taken through its `active` link so the path
/// stays right across updates: from /.flatpak-info's app-path
/// (…/<arch>/<branch>/<commit>/files). None outside Flatpak, when the files
/// are missing, or when the path has a character the unit file or the shell
/// would read otherwise.
pub fn tessera_p11kit_dir() -> Option<String> {
    let info = fs::read_to_string("/.flatpak-info").ok()?;
    let app_path = info.lines().find_map(|l| l.strip_prefix("app-path="))?;
    let exe = std::env::current_exe().ok()?;
    let inside = exe.parent()?.parent()?.join("lib/tessera/p11-kit");
    if !inside.join("p11-kit").exists() {
        return None;
    }
    p11kit_dir_from(app_path, &inside)
}

fn p11kit_dir_from(app_path: &str, inside: &std::path::Path) -> Option<String> {
    let files = std::path::Path::new(app_path.trim());
    if files.file_name()? != "files" {
        return None;
    }
    let below_app = inside.strip_prefix("/app").ok()?;
    let branch = files.parent()?.parent()?;
    let dir = branch.join("active/files").join(below_app);
    let dir = dir.to_str()?.to_string();
    let safe = dir.starts_with('/')
        && dir
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c));
    safe.then_some(dir)
}

/// The command for this host (see SOCKET_COMMAND, debian_command,
/// hub_command and LEGACY_COMMAND), by the release (native.rs, host_p11kit):
/// what the user's server should be does not change with what answers now.
pub fn host_server_command() -> String {
    if !crate::native::host_is_debian_family() {
        return SOCKET_COMMAND.to_string();
    }
    let p11kit = crate::native::host_p11kit(None);
    let hub = if p11kit == crate::native::HostP11kit::Fixable {
        tessera_p11kit_dir()
    } else {
        None
    };
    debian_command_for(p11kit, hub.as_deref())
}

fn debian_command_for(p11kit: crate::native::HostP11kit, hub: Option<&str>) -> String {
    use crate::native::HostP11kit::*;
    match (p11kit, hub) {
        (Fine, _) => debian_command(),
        (Fixable, Some(dir)) => hub_command(dir),
        (Fixable, None) | (Unsupported, _) => LEGACY_COMMAND.to_string(),
    }
}

/// Is the command too long to show in the window (it is copied instead)?
pub fn host_server_command_is_long() -> bool {
    crate::native::host_is_debian_family()
}

/// The command that lets a sandboxed app reach the host's p11-kit server.
pub fn override_command(app: &ClientApp) -> String {
    format!(
        "flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 {}",
        app.id
    )
}

/// The NSS database a Chromium-based browser (or Evolution) reads: since Chromium 146,
/// $XDG_DATA_HOME/pki/nssdb (in Flatpak, ~/.var/app/<id>/data/pki/nssdb),
/// unless the older ~/.pki/nssdb exists, which then wins. ~/.pki is the real
/// one for Chromium, which may use the home folder, and the one kept by
/// --persist=.pki for the others. None until the browser has made one
/// (Tessera sees the folder only if it existed when Tessera started).
pub fn chromium_nssdb(app: &ClientApp) -> Option<std::path::PathBuf> {
    let data = paths::home().join(".var/app").join(app.id);
    let legacy = if app.id == CHROMIUM.id || app.id == EVOLUTION.id {
        paths::home().join(".pki/nssdb")
    } else {
        data.join(".pki/nssdb")
    };
    [legacy, data.join("data/pki/nssdb")]
        .into_iter()
        .find(|d| d.is_dir())
}

/// Has `flatpak override --user` given this app the p11-kit socket?
pub fn has_socket_override(app_id: &str) -> bool {
    let Ok(text) = fs::read_to_string(paths::user_override(app_id)) else {
        return false;
    };
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("filesystems="))
        .flat_map(|v| v.split(';'))
        .any(|f| f.trim() == "xdg-run/p11-kit/pkcs11")
}

/// Does the host's p11-kit server answer?
pub fn host_server_answers() -> bool {
    std::os::unix::net::UnixStream::connect(paths::host_p11kit_socket()).is_ok()
}

fn host_module_name(id: &str) -> String {
    format!("tessera-{id}.module")
}

/// A driver without a token is listed after every other module. Through
/// p11-kit's RPC, a module whose slot list is empty answers
/// C_GetSlotList(CK_TRUE, buffer, &0) with CKR_BUFFER_TOO_SMALL (the
/// server sends a NULL array for a 0-length one, which the client then
/// takes for a buffer too small). The system's p11-kit server walks the
/// modules in order with p11_kit_iter, which stops at the first error: every
/// module after an empty one of Tessera's was missing for the apps (OpenSC
/// with no card hid the GD Burti's token, say). p11-kit orders the modules by
/// `priority:`, highest first (0 when unset; its trust module has 1), then by
/// name; with -1 the empty ones come last, where they cut nothing off.
pub const EMPTY_PRIORITY: &str = "-1";

fn host_module_contents(d: &Driver, has_token: bool) -> String {
    let priority = if has_token {
        String::new()
    } else {
        format!("priority: {EMPTY_PRIORITY}\n")
    };
    format!(
        "# Written by Tessera ({}): {}, served from its sandbox.\nremote: unix:path={}\n{priority}",
        paths::APP_ID,
        d.name,
        paths::driver_socket(&d.id).display()
    )
}

/// Write one host module file per running driver and remove Tessera's stale
/// ones; those of `with_tokens` (ids) come first for the apps, the others
/// last (see EMPTY_PRIORITY). Returns how many are in place.
pub fn write_host_modules(
    drivers: &[Driver],
    running: &[String],
    with_tokens: &[String],
) -> Result<usize> {
    let dir = paths::host_modules_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let wanted: Vec<&Driver> = drivers.iter().filter(|d| running.contains(&d.id)).collect();
    for entry in fs::read_dir(&dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("tessera-")
            && name.ends_with(".module")
            && !wanted.iter().any(|d| host_module_name(&d.id) == name)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
    for d in &wanted {
        let path = dir.join(host_module_name(&d.id));
        let body = host_module_contents(d, with_tokens.contains(&d.id));
        if fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
            fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    Ok(wanted.len())
}

/// The system's own OpenSC, if it has one, is switched off for the system's
/// p11-kit, from Tessera's first start, unless the user turns it back on in
/// the window; it stays off with Tessera closed. With a G+D StarSign token
/// (the GD Burti) the system's OpenSC hangs the token, and GNOME's session
/// with it, whenever anything opens it through p11-kit; Tessera's own OpenSC
/// serves the same cards and leaves those tokens alone (see
/// flatpak/drivers/opensc). p11-kit reads a module's file in the user's
/// folder first, and takes from the system's file of the same name only
/// what the user's does not set: a user's file whose `enable-in:` names no
/// real program turns the module off for every program that starts after
/// it (p11-kit-proxy.so in native apps, and each connection to the system's
/// p11-kit server, through which Flatpak apps reach it), without touching
/// the system's file. Fedora and Arch name it opensc.module, Debian and
/// Ubuntu opensc-pkcs11.module; a file for a module the system does not
/// have does nothing.
const HOST_OPENSC_FILES: [&str; 2] = ["opensc.module", "opensc-pkcs11.module"];

/// The first line of Tessera's files: only files that start with it are
/// changed or removed (a file of the user's own is left as it is).
const HOST_OPENSC_MARK: &str =
    "# Written by Tessera (io.github.pedrohqb.Tessera): the system's OpenSC is off; Tessera's own serves the same cards.";

fn host_opensc_contents() -> String {
    format!(
        "{HOST_OPENSC_MARK}\n# To turn it back on: in Tessera's window (Service section), or delete this file.\nenable-in: tessera-turned-it-off\n"
    )
}

/// Switch the system's OpenSC off (`off`) or back on, through Tessera's own
/// files only.
pub fn set_host_opensc_off(off: bool) -> Result<()> {
    set_host_opensc_off_in(&paths::host_modules_dir(), off)
}

fn set_host_opensc_off_in(dir: &std::path::Path, off: bool) -> Result<()> {
    if off {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    for name in HOST_OPENSC_FILES {
        let path = dir.join(name);
        let ours = match fs::read_to_string(&path) {
            Ok(text) => text.starts_with(HOST_OPENSC_MARK),
            Err(_) if !path.exists() => true,
            Err(_) => false,
        };
        if !ours {
            continue;
        }
        if off {
            let body = host_opensc_contents();
            if fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
                fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
            }
        } else if path.exists() {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A driver with a token keeps p11-kit's default place; one without
    /// goes after every other module (EMPTY_PRIORITY).
    #[test]
    fn empty_drivers_come_last() {
        let d = Driver {
            id: "opensc".into(),
            name: "OpenSC".into(),
            vendor: String::new(),
            description: String::new(),
            module: "/x.so".into(),
            tools: vec![],
            create_dirs: vec![],
            library_path: vec![],
            env: Default::default(),
            when_on: Default::default(),
            usb: false,
            pcscd_ignore: None,
            usb_id: None,
            patient: false,
            shared: false,
        };
        let with = host_module_contents(&d, true);
        assert!(with.contains("remote: unix:path="));
        assert!(!with.contains("priority"));
        let without = host_module_contents(&d, false);
        assert!(without.ends_with("\npriority: -1\n"), "{without}");
    }

    #[test]
    fn server_command_by_distribution() {
        use crate::native::HostP11kit::*;
        assert_eq!(debian_command_for(Fine, None), debian_command());
        assert_eq!(debian_command_for(Fine, Some("/x")), debian_command());
        assert_eq!(debian_command_for(Fixable, Some("/x")), hub_command("/x"));
        assert_eq!(debian_command_for(Fixable, None), LEGACY_COMMAND);
        assert_eq!(debian_command_for(Unsupported, Some("/x")), LEGACY_COMMAND);
    }

    #[test]
    fn tessera_p11kit_dir_from_the_app_path() {
        let id = "io.github.pedrohqb.Tessera";
        let own = std::path::Path::new("/app/lib/tessera/p11-kit");
        let flatpark = std::path::Path::new("/app/extra/tessera/lib/tessera/p11-kit");
        assert_eq!(
            p11kit_dir_from(
                &format!("/var/lib/flatpak/app/{id}/x86_64/stable/0a1b2c/files"),
                own
            ),
            Some(format!(
                "/var/lib/flatpak/app/{id}/x86_64/stable/active/files/lib/tessera/p11-kit"
            ))
        );
        assert_eq!(
            p11kit_dir_from(
                &format!("/home/ana/.local/share/flatpak/app/{id}/x86_64/stable/ff/files\n"),
                flatpark
            ),
            Some(format!(
                "/home/ana/.local/share/flatpak/app/{id}/x86_64/stable/active/files/extra/tessera/lib/tessera/p11-kit"
            ))
        );
        // Not under /app (a build run outside Flatpak).
        assert_eq!(
            p11kit_dir_from(
                &format!("/var/lib/flatpak/app/{id}/x86_64/stable/c/files"),
                std::path::Path::new("/usr/lib/tessera/p11-kit")
            ),
            None
        );
        // Characters systemd or the shell would read (a space, `%`, a quote).
        for bad in [
            "/home/ana maria/.local/share/flatpak/app/x/x86_64/stable/c/files",
            "/home/100%/.local/share/flatpak/app/x/x86_64/stable/c/files",
            "/home/o'neil/.local/share/flatpak/app/x/x86_64/stable/c/files",
            "/var/lib/flatpak/app/x/x86_64/stable/c",
            "",
        ] {
            assert_eq!(p11kit_dir_from(bad, own), None, "{bad}");
        }
    }

    /// The hub command writes upstream's two units, running Tessera's
    /// p11-kit, with `%t` intact and the helpers' folder in the service's
    /// environment (the systemctl part is left out: there is no user
    /// systemd here).
    #[test]
    fn hub_command_writes_the_units() {
        let dir = "/var/lib/flatpak/app/io.github.pedrohqb.Tessera/x86_64/stable/active/files/lib/tessera/p11-kit";
        let cmd = hub_command(dir);
        let home = write_units(&cmd, "hub-units");
        let units = home.join(".config/systemd/user");
        assert_eq!(
            fs::read_to_string(units.join("p11-kit-server.socket")).unwrap(),
            "[Unit]\nDescription=p11-kit server (Tessera's p11-kit)\n\
             [Socket]\nPriority=6\nBacklog=5\nListenStream=%t/p11-kit/pkcs11\nSocketMode=0600\n\
             [Install]\nWantedBy=sockets.target\n"
        );
        assert_eq!(
            fs::read_to_string(units.join("p11-kit-server.service")).unwrap(),
            format!(
                "[Unit]\nDescription=p11-kit server (Tessera's p11-kit)\nDocumentation=man:p11-kit(8)\n\
                 Requires=p11-kit-server.socket\n\
                 [Service]\nType=simple\nEnvironment=P11_KIT_PRIVATEDIR={dir}\n\
                 ExecStart={dir}/p11-kit server -f pkcs11:\nRestart=on-failure\n\
                 [Install]\nAlso=p11-kit-server.socket\nWantedBy=default.target\n"
            )
        );
        // The running server is replaced before the socket is made, and
        // Tessera opens again on the new socket.
        let stop = cmd.find("stop p11-kit-server.service").unwrap();
        let start = cmd.find("start p11-kit-server.socket").unwrap();
        assert!(stop < start);
        assert!(cmd.ends_with(
            "{ flatpak kill io.github.pedrohqb.Tessera 2>/dev/null; (setsid flatpak run io.github.pedrohqb.Tessera >/dev/null 2>&1 &); }"
        ));
        let _ = fs::remove_dir_all(&home);
    }

    /// Runs the part of a command that writes the units (the systemctl part
    /// is left out: there is no user systemd here), in a made-up home, and
    /// returns the folder.
    fn write_units(cmd: &str, name: &str) -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!("tessera-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let write = cmd.split(" && systemctl").next().unwrap();
        let ok = std::process::Command::new("sh")
            .arg("-c")
            .arg(write)
            .env("HOME", &home)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        home
    }

    /// Ubuntu 26.04's command: upstream's units, running the system's
    /// p11-kit, without `-n`.
    #[test]
    fn debian_command_writes_the_units() {
        let cmd = debian_command();
        let home = write_units(&cmd, "debian-units");
        let units = home.join(".config/systemd/user");
        assert_eq!(
            fs::read_to_string(units.join("p11-kit-server.socket")).unwrap(),
            "[Unit]\nDescription=p11-kit server\n\
             [Socket]\nPriority=6\nBacklog=5\nListenStream=%t/p11-kit/pkcs11\nSocketMode=0600\n\
             [Install]\nWantedBy=sockets.target\n"
        );
        assert_eq!(
            fs::read_to_string(units.join("p11-kit-server.service")).unwrap(),
            "[Unit]\nDescription=p11-kit server\nDocumentation=man:p11-kit(8)\n\
             Requires=p11-kit-server.socket\n\
             [Service]\nType=simple\n\
             ExecStart=/usr/bin/p11-kit server -f pkcs11:\nRestart=on-failure\n\
             [Install]\nAlso=p11-kit-server.socket\nWantedBy=default.target\n"
        );
        let stop = cmd.find("stop p11-kit-server.service").unwrap();
        let start = cmd.find("start p11-kit-server.socket").unwrap();
        assert!(stop < start);
        let _ = fs::remove_dir_all(&home);
    }

    /// The plain service, for a p11-kit that takes no socket from systemd.
    #[test]
    fn legacy_command_writes_the_service() {
        let home = write_units(LEGACY_COMMAND, "legacy-unit");
        let unit =
            fs::read_to_string(home.join(".config/systemd/user/p11-kit-server.service")).unwrap();
        assert_eq!(
            unit,
            "[Unit]\nDescription=p11-kit server\nDocumentation=man:p11-kit(8)\n\
             [Service]\nExecStartPre=/bin/mkdir -p -m 700 %t/p11-kit\n\
             ExecStart=/usr/bin/p11-kit server -f -n %t/p11-kit/pkcs11 pkcs11:\n\
             Restart=on-failure\n[Install]\nWantedBy=default.target\n"
        );
        let _ = fs::remove_dir_all(&home);
    }

    /// The system's OpenSC goes off through Tessera's own files, which go
    /// away again; a file of the user's own is never changed.
    #[test]
    fn the_systems_opensc_is_switched_off_and_on() {
        let dir = std::env::temp_dir().join(format!("tessera-opensc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        set_host_opensc_off_in(&dir, true).unwrap();
        for name in HOST_OPENSC_FILES {
            let text = fs::read_to_string(dir.join(name)).unwrap();
            assert!(text.starts_with(HOST_OPENSC_MARK));
            assert!(text.contains("\nenable-in: tessera-turned-it-off\n"));
        }
        set_host_opensc_off_in(&dir, false).unwrap();
        assert!(HOST_OPENSC_FILES.iter().all(|n| !dir.join(n).exists()));
        // The user's own file stays as it is, on and off.
        let mine = "# mine\nmodule: /opt/opensc-pkcs11.so\n";
        fs::write(dir.join("opensc.module"), mine).unwrap();
        set_host_opensc_off_in(&dir, true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("opensc.module")).unwrap(), mine);
        assert!(dir.join("opensc-pkcs11.module").exists());
        set_host_opensc_off_in(&dir, false).unwrap();
        assert_eq!(fs::read_to_string(dir.join("opensc.module")).unwrap(), mine);
        assert!(!dir.join("opensc-pkcs11.module").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
