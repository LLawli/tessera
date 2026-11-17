//! TESSERA_ENCRYPT_FLAG=1: C_GetMechanismInfo also says CKF_ENCRYPT for the
//! mechanisms the driver says decrypt but do not encrypt.
//!
//! OpenSC reports its RSA mechanisms (CKM_RSA_PKCS, CKM_RSA_X_509) as
//! sign, verify and decrypt, without encrypt: a card holds the private key,
//! and encrypting with the public one is done in software. Java's SunPKCS11
//! takes a mechanism that decrypts and does not encrypt for a half
//! supported, "legacy" one and drops it whole, signing included
//! (JDK-8176837; only later versions check per use, JDK-8293345, or have
//! `allowLegacy`). PJeOffice (signer4j) signs with NONEwithRSA, which is
//! CKM_RSA_PKCS: with it dropped, Java went on to the next provider,
//! BouncyCastle, which cannot use a token's key ("Supplied key
//! (sun.security.pkcs11.P11Key$P11PrivateKey) is not a RSAPrivateKey
//! instance"), and the token was never asked to sign: after C_Login the
//! trace had no C_SignInit at all. Firefox, which is not Java, signed.
//!
//! The flag is not true: C_EncryptInit still fails in the driver. Apps
//! encrypt with a public key in software, so nothing was seen to ask.
//!
//! Tessera's OpenSC driver sets it (`[env]` in its driver.toml); another
//! driver.toml can.

use std::ffi::{c_ulong, c_void};
use std::sync::OnceLock;

type GetMechanismInfo = unsafe extern "C" fn(c_ulong, c_ulong, *mut c_ulong) -> c_ulong;

/// Index of C_GetMechanismInfo among the functions of CK_FUNCTION_LIST.
const GET_MECHANISM_INFO: usize = 8;
const CKF_ENCRYPT: c_ulong = 0x100;
const CKF_DECRYPT: c_ulong = 0x200;

static ORIGINAL: OnceLock<usize> = OnceLock::new();

pub fn wanted() -> bool {
    std::env::var("TESSERA_ENCRYPT_FLAG").as_deref() == Ok("1")
}

/// The flags to report for a mechanism whose driver reports `flags`.
pub fn adjusted(flags: c_ulong) -> c_ulong {
    if flags & CKF_DECRYPT != 0 {
        flags | CKF_ENCRYPT
    } else {
        flags
    }
}

/// C_GetMechanismInfo, then the flags adjusted.
///
/// # Safety
///
/// Called by p11-kit as the driver's C_GetMechanismInfo, with its
/// arguments: `info` is null or points to a CK_MECHANISM_INFO the caller
/// owns (three CK_ULONGs: ulMinKeySize, ulMaxKeySize, flags). ORIGINAL was
/// set by `wrap` before this list was handed out and is the driver's own
/// C_GetMechanismInfo. The flags are changed only when the driver returned
/// CKR_OK, so it filled the structure.
unsafe extern "C" fn get_mechanism_info(
    slot: c_ulong,
    mechanism: c_ulong,
    info: *mut c_ulong,
) -> c_ulong {
    let orig: GetMechanismInfo =
        std::mem::transmute::<usize, GetMechanismInfo>(*ORIGINAL.get().unwrap());
    let rv = orig(slot, mechanism, info);
    if rv == 0 && !info.is_null() {
        *info.add(2) = adjusted(*info.add(2));
    }
    rv
}

/// A copy of `module` whose C_GetMechanismInfo is the one above.
///
/// # Safety
///
/// As serial::wrap: `module` points to a valid CK_FUNCTION_LIST (2.x) or
/// CK_FUNCTION_LIST_3_0, whose first word holds the CK_VERSION and each of
/// the following words a function pointer. The copy is leaked on purpose:
/// p11-kit uses it until the process ends. Call it once per process.
pub unsafe fn wrap(module: *mut c_void) -> *mut c_void {
    let version = *(module as *const u8);
    let words = 1 + if version >= 3 { 92 } else { 68 };
    let mut copy = vec![0usize; words];
    std::ptr::copy_nonoverlapping(module as *const usize, copy.as_mut_ptr(), words);
    if copy[1 + GET_MECHANISM_INFO] == 0 {
        return module;
    }
    let _ = ORIGINAL.set(copy[1 + GET_MECHANISM_INFO]);
    copy[1 + GET_MECHANISM_INFO] = get_mechanism_info as *const () as usize;
    Box::leak(copy.into_boxed_slice()).as_mut_ptr() as *mut c_void
}

#[cfg(test)]
mod tests {
    use super::*;

    const CKF_HW: c_ulong = 0x1;
    const CKF_SIGN: c_ulong = 0x800;
    const CKF_VERIFY: c_ulong = 0x2000;

    #[test]
    fn a_mechanism_that_decrypts_also_says_it_encrypts() {
        // OpenSC's CKM_RSA_PKCS.
        let opensc = CKF_HW | CKF_SIGN | CKF_VERIFY | CKF_DECRYPT;
        assert_eq!(adjusted(opensc), opensc | CKF_ENCRYPT);
        // Its CKM_SHA256_RSA_PKCS and CKM_RSA_PKCS_PSS: signing only.
        let sign = CKF_HW | CKF_SIGN | CKF_VERIFY;
        assert_eq!(adjusted(sign), sign);
        let both = CKF_ENCRYPT | CKF_DECRYPT;
        assert_eq!(adjusted(both), both);
    }

    static mut FAKE: [usize; 69] = [0; 69];

    unsafe extern "C" fn fake_info(_: c_ulong, mechanism: c_ulong, info: *mut c_ulong) -> c_ulong {
        if mechanism != 1 {
            return 0x70; // CKR_MECHANISM_INVALID
        }
        *info = 1024;
        *info.add(1) = 2048;
        *info.add(2) = CKF_HW | CKF_SIGN | CKF_VERIFY | CKF_DECRYPT;
        0
    }

    /// Through the wrapped list: the flag on what the driver answered, the
    /// key sizes as they were, and nothing written when it refused.
    #[test]
    fn the_wrapped_list_reports_the_flag() {
        // SAFETY: FAKE is a function list of the 2.x shape (a version word
        // and 68 function words) used only by this test; the wrapped copy's
        // C_GetMechanismInfo is called with a three-word structure.
        unsafe {
            let list = std::ptr::addr_of_mut!(FAKE) as *mut usize;
            *(list as *mut u8) = 2;
            *list.add(1 + GET_MECHANISM_INFO) = fake_info as *const () as usize;
            let wrapped = wrap(list as *mut c_void) as *const usize;
            let f: GetMechanismInfo = std::mem::transmute::<usize, GetMechanismInfo>(
                *wrapped.add(1 + GET_MECHANISM_INFO),
            );
            let mut info: [c_ulong; 3] = [0; 3];
            assert_eq!(f(0, 1, info.as_mut_ptr()), 0);
            assert_eq!(info[..2], [1024, 2048]);
            assert_eq!(
                info[2],
                CKF_HW | CKF_SIGN | CKF_VERIFY | CKF_DECRYPT | CKF_ENCRYPT
            );
            let mut none: [c_ulong; 3] = [7; 3];
            assert_eq!(f(0, 2, none.as_mut_ptr()), 0x70);
            assert_eq!(none, [7; 3]);
        }
    }
}
