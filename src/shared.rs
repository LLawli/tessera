//! `tessera shared --socket PATH --provider MODULE`: one process that loads
//! a driver once and serves every connection to its socket, for the drivers
//! with `usb = true` (drivers.rs) instead of `p11-kit server`.
//!
//! Why: p11-kit server starts a process per connection, each loading the
//! driver anew. Feitian's ePass2003 library talks to the token over USB
//! itself and reads the whole token on its first C_GetTokenInfo (11 s in
//! the first test), so every listing and every app paid that, Tessera's
//! listing ran out of time and was stopped in the middle of a USB transfer
//! (which hangs the token until it is unplugged), and two apps meant two
//! processes holding the same USB device. Here the driver is loaded and
//! initialized once, its token read once at start (warm-up), and only this
//! process talks to the device.
//!
//! Each connection is served on its own thread by p11-kit's
//! p11_kit_remote_serve_tokens, as `tessera remote` does on its stdin, over
//! a copy of the driver's function list (`wrap`) that:
//! - takes C_Initialize and C_Finalize per connection: the driver stays
//!   initialized, and an app that finalizes closes only its own sessions;
//! - keeps each connection's sessions apart, so C_CloseAllSessions and the
//!   end of a connection close that connection's sessions only;
//! - lets one call at a time into the driver (one token, one USB pipe).
//!
//! The PKCS#11 login state belongs to the process, so it is shared by the
//! apps: once one app has logged in, the others find the token logged in
//! (an app that logs in again gets CKR_USER_ALREADY_LOGGED_IN, which apps
//! take as success), and one app's C_Logout logs out all.

use libloading::{Library, Symbol};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_ulong, c_void, CString};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

type CkRv = c_ulong;
type Fn6 = unsafe extern "C" fn(usize, usize, usize, usize, usize, usize) -> CkRv;
type GetFunctionList = unsafe extern "C" fn(*mut *mut c_void) -> CkRv;
type ServeTokens =
    unsafe extern "C" fn(*const *const c_char, usize, *mut c_void, c_int, c_int) -> c_int;

const CKR_OK: CkRv = 0;
const CKR_CRYPTOKI_ALREADY_INITIALIZED: CkRv = 0x191;
const CKF_OS_LOCKING_OK: c_ulong = 2;

const C_INITIALIZE: usize = 0;
const C_FINALIZE: usize = 1;
const C_GET_FUNCTION_LIST: usize = 3;
const C_GET_SLOT_LIST: usize = 4;
const C_GET_TOKEN_INFO: usize = 6;
const C_OPEN_SESSION: usize = 12;
const C_CLOSE_SESSION: usize = 13;
const C_CLOSE_ALL_SESSIONS: usize = 14;
/// C_GenerateKeyPair and C_UnwrapKey take eight arguments (see trace.rs):
/// passed through as they are, without the lock.
const EIGHT_ARGS: [usize; 2] = [59, 61];
/// C_WaitForSlotEvent may block: passed through without the lock.
const C_WAIT_FOR_SLOT_EVENT: usize = 67;

/// The name of the threads serving a connection; server.rs counts them to
/// tell whether an app is connected.
pub const CONNECTION_THREAD: &str = "tessera-conn";

/// The wrapped list's functions (the driver's, through stable.rs and the
/// others), and the list handed out.
static ORIGINAL: OnceLock<Vec<usize>> = OnceLock::new();
static SERVED: OnceLock<usize> = OnceLock::new();
/// One call at a time into the driver.
static LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// The sessions this thread's connection opened: (session, slot).
    static SESSIONS: RefCell<Vec<(c_ulong, c_ulong)>> = const { RefCell::new(Vec::new()) };
}

/// Function I of the wrapped list, called under the lock.
///
/// # Safety
///
/// ORIGINAL holds the function list given to `wrap`. As in trace.rs, every
/// function sent here takes at most six arguments, each an integer or a
/// pointer, which on x86-64 arrive in registers: forwarding six usize
/// values passes them unchanged, and the callee ignores the extra ones.
unsafe fn call(i: usize, a: [usize; 6]) -> CkRv {
    let f: Fn6 = std::mem::transmute::<usize, Fn6>(ORIGINAL.get().expect("wrapped")[i]);
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f(a[0], a[1], a[2], a[3], a[4], a[5])
}

/// # Safety
///
/// See `call`; called by p11-kit with the arguments of function I.
unsafe extern "C" fn tramp<const I: usize>(
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
) -> CkRv {
    call(I, [a, b, c, d, e, f])
}

/// The driver was initialized when the process started.
unsafe extern "C" fn initialize(_args: usize) -> CkRv {
    CKR_OK
}

/// Close this connection's sessions; the driver stays initialized.
unsafe extern "C" fn finalize(_reserved: usize) -> CkRv {
    end_connection();
    CKR_OK
}

/// # Safety
///
/// `out` is null or points to a writable CK_FUNCTION_LIST_PTR.
unsafe extern "C" fn get_function_list(out: *mut usize) -> CkRv {
    if out.is_null() {
        return 7; // CKR_ARGUMENTS_BAD
    }
    *out = *SERVED.get().expect("wrapped");
    CKR_OK
}

/// # Safety
///
/// The arguments of C_OpenSession; `session` is valid when it returns OK.
unsafe extern "C" fn open_session(
    slot: usize,
    flags: usize,
    app: usize,
    notify: usize,
    session: usize,
    _f: usize,
) -> CkRv {
    let rv = call(C_OPEN_SESSION, [slot, flags, app, notify, session, 0]);
    if rv == CKR_OK && session != 0 {
        let h = *(session as *const c_ulong);
        SESSIONS.with(|s| s.borrow_mut().push((h, slot as c_ulong)));
    }
    rv
}

unsafe extern "C" fn close_session(session: usize) -> CkRv {
    let rv = call(C_CLOSE_SESSION, [session, 0, 0, 0, 0, 0]);
    SESSIONS.with(|s| s.borrow_mut().retain(|(h, _)| *h != session as c_ulong));
    rv
}

/// This connection's sessions on the slot, not every app's.
unsafe extern "C" fn close_all_sessions(slot: usize) -> CkRv {
    let mine: Vec<c_ulong> = SESSIONS.with(|s| {
        let mut s = s.borrow_mut();
        let (on_slot, rest): (Vec<_>, Vec<_>) =
            s.drain(..).partition(|(_, sl)| *sl == slot as c_ulong);
        *s = rest;
        on_slot.into_iter().map(|(h, _)| h).collect()
    });
    for h in mine {
        call(C_CLOSE_SESSION, [h as usize, 0, 0, 0, 0, 0]);
    }
    CKR_OK
}

/// Close the sessions this thread's connection left open.
pub fn end_connection() {
    let mine: Vec<c_ulong> = SESSIONS.with(|s| s.borrow_mut().drain(..).map(|(h, _)| h).collect());
    for h in mine {
        // SAFETY: ORIGINAL is set (sessions are only recorded through the
        // wrapped list); C_CloseSession takes one integer.
        unsafe {
            call(C_CLOSE_SESSION, [h as usize, 0, 0, 0, 0, 0]);
        }
    }
}

macro_rules! table {
    ($($i:literal)*) => { [$(tramp::<$i> as *const () as usize),*] };
}

/// A copy of `module` for every connection (see the module's comment).
///
/// # Safety
///
/// As trace::wrap: `module` points to a valid CK_FUNCTION_LIST (2.x, 68
/// functions) or CK_FUNCTION_LIST_3_0 (92), first word the version, the
/// others function pointers; only the first 68 are wrapped. The copy is
/// leaked on purpose, and wrap is called once per process.
pub unsafe fn wrap(module: *mut c_void) -> *mut c_void {
    let version = *(module as *const u8);
    let words = 1 + if version >= 3 { 92 } else { 68 };
    let mut copy = vec![0usize; words];
    std::ptr::copy_nonoverlapping(module as *const usize, copy.as_mut_ptr(), words);
    let _ = ORIGINAL.set(copy[1..69].to_vec());
    let tramps: [usize; 68] = table!(
        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29
        30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56
        57 58 59 60 61 62 63 64 65 66 67
    );
    for i in 0..68 {
        if copy[1 + i] == 0 || EIGHT_ARGS.contains(&i) || i == C_WAIT_FOR_SLOT_EVENT {
            continue;
        }
        copy[1 + i] = match i {
            C_INITIALIZE => initialize as *const () as usize,
            C_FINALIZE => finalize as *const () as usize,
            C_GET_FUNCTION_LIST => get_function_list as *const () as usize,
            C_OPEN_SESSION => open_session as *const () as usize,
            C_CLOSE_SESSION => close_session as *const () as usize,
            C_CLOSE_ALL_SESSIONS => close_all_sessions as *const () as usize,
            _ => tramps[i],
        };
    }
    let list = Box::leak(copy.into_boxed_slice()).as_mut_ptr() as *mut c_void;
    let _ = SERVED.set(list as usize);
    list
}

#[repr(C)]
struct InitArgs {
    create_mutex: usize,
    destroy_mutex: usize,
    lock_mutex: usize,
    unlock_mutex: usize,
    flags: c_ulong,
    reserved: usize,
}

/// Read the token once at start: the driver reads it whole on the first
/// C_GetTokenInfo, so the first listing and the first app do not wait.
/// Logged with timings, to the driver's log.
///
/// # Safety
///
/// `served` is the list `wrap` returned.
unsafe fn warm_up(served: usize) {
    let f = |i: usize| -> Fn6 {
        std::mem::transmute::<usize, Fn6>(*(served as *const usize).add(1 + i))
    };
    for round in 1..=2 {
        let t = Instant::now();
        let mut slots = [0 as c_ulong; 16];
        let mut n: c_ulong = slots.len() as c_ulong;
        let rv = f(C_GET_SLOT_LIST)(
            1,
            slots.as_mut_ptr() as usize,
            &mut n as *mut c_ulong as usize,
            0,
            0,
            0,
        );
        eprintln!(
            "tessera shared: warm-up {round}: C_GetSlotList = {rv:#x}, {n} slot(s) with a token [{} ms]",
            t.elapsed().as_millis()
        );
        if rv != CKR_OK {
            return;
        }
        for &slot in &slots[..(n as usize).min(slots.len())] {
            let t = Instant::now();
            // CK_TOKEN_INFO is 208 bytes on x86-64.
            let mut info = [0u8; 256];
            let rv = f(C_GET_TOKEN_INFO)(slot as usize, info.as_mut_ptr() as usize, 0, 0, 0, 0);
            eprintln!(
                "tessera shared: warm-up {round}: C_GetTokenInfo(slot {slot}) = {rv:#x} [{} ms]",
                t.elapsed().as_millis()
            );
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut provider: Option<&str> = None;
    let mut socket: Option<&str> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--provider" => provider = it.next().map(String::as_str),
            "--socket" => socket = it.next().map(String::as_str),
            other => {
                eprintln!("tessera shared: unknown argument {other}");
                return 2;
            }
        }
    }
    let (Some(provider), Some(socket)) = (provider, socket) else {
        eprintln!("usage: tessera shared --socket PATH --provider MODULE");
        return 2;
    };
    // SAFETY:
    // - The libp11-kit symbol is looked up by name and called with the
    //   signature of p11-kit/remote.h (p11_kit_remote_serve_tokens); `p11`
    //   and `driver` stay loaded for the life of the process (leaked).
    // - C_GetFunctionList has the PKCS#11 signature and fills `list`,
    //   checked for null; C_Initialize gets a CK_C_INITIALIZE_ARGS that
    //   lives through the call.
    // - stable::wrap, serial::wrap, trace::wrap and wrap copy the list, each
    //   once per process.
    unsafe {
        let p11: &'static Library = match Library::new("libp11-kit.so.0") {
            Ok(l) => Box::leak(Box::new(l)),
            Err(e) => {
                eprintln!("tessera shared: {e}");
                return 1;
            }
        };
        let serve: Symbol<ServeTokens> = match p11.get(b"p11_kit_remote_serve_tokens\0") {
            Ok(s) => s,
            Err(_) => {
                eprintln!("tessera shared: libp11-kit lacks the remote API");
                return 1;
            }
        };
        let serve: ServeTokens = *serve;
        let driver: &'static Library = match Library::new(provider) {
            Ok(l) => Box::leak(Box::new(l)),
            Err(e) => {
                eprintln!("tessera shared: {e}");
                return 1;
            }
        };
        let mut list: *mut c_void = std::ptr::null_mut();
        let rv = match driver.get::<GetFunctionList>(b"C_GetFunctionList\0") {
            Ok(f) => f(&mut list),
            Err(_) => 1,
        };
        if rv != CKR_OK || list.is_null() {
            eprintln!("tessera shared: {provider} has no function list ({rv:#x})");
            return 1;
        }
        let t = Instant::now();
        let init_args = InitArgs {
            create_mutex: 0,
            destroy_mutex: 0,
            lock_mutex: 0,
            unlock_mutex: 0,
            flags: CKF_OS_LOCKING_OK,
            reserved: 0,
        };
        let init: unsafe extern "C" fn(*const InitArgs) -> CkRv =
            std::mem::transmute::<usize, _>(*(list as *const usize).add(1 + C_INITIALIZE));
        let rv = init(&init_args);
        eprintln!(
            "tessera shared: {provider}: C_Initialize = {rv:#x} [{} ms]",
            t.elapsed().as_millis()
        );
        if rv != CKR_OK && rv != CKR_CRYPTOKI_ALREADY_INITIALIZED {
            return 1;
        }
        let served = crate::stable::wrap(list);
        let served = match std::env::var("TESSERA_TOKEN_SERIAL")
            .ok()
            .and_then(|s| crate::serial::field(&s))
        {
            Some(serial) => crate::serial::wrap(served, serial),
            None => served,
        };
        // PSS made with raw RSA, for drivers that fail it (see pss.rs).
        let served = if crate::pss::wanted() {
            crate::pss::wrap(served)
        } else {
            served
        };
        let served = if std::env::var("TESSERA_REMOTE_TRACE").as_deref() == Ok("1") {
            crate::trace::wrap(served)
        } else {
            served
        };
        let served = wrap(served) as usize;

        let path = Path::new(socket);
        let _ = std::fs::remove_file(path);
        let listener = match UnixListener::bind(path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("tessera shared: {socket}: {e}");
                return 1;
            }
        };
        eprintln!("tessera shared: serving {provider} at {socket}");
        std::thread::spawn(move || warm_up(served));

        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let spawned = std::thread::Builder::new()
                .name(CONNECTION_THREAD.into())
                .spawn(move || {
                    let any = CString::new("pkcs11:").expect("no NUL");
                    let tokens = [any.as_ptr()];
                    let fd = stream.as_raw_fd();
                    // p11_kit_remote_serve_tokens reads and writes the
                    // descriptor and leaves it open; `stream` closes it.
                    serve(tokens.as_ptr(), 1, served as *mut c_void, fd, fd);
                    end_connection();
                    drop(stream);
                });
            if let Err(e) = spawned {
                eprintln!("tessera shared: {e}");
            }
        }
    }
    0
}
