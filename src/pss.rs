//! RSA-PSS for drivers that announce it but fail it (TESSERA_PSS_VIA_RAW=1
//! in the driver's environment).
//!
//! TLS 1.3 signs with RSA-PSS (CKM_RSA_PKCS_PSS on a hash the app computed).
//! Feitian's ePass2003 library lists the mechanism, but its C_SignInit fails
//! with CKR_GENERAL_ERROR, so Firefox's handshake with a client certificate
//! failed after the PIN. The token does raw RSA (CKM_RSA_X_509) correctly:
//! a block padded by hand for PKCS#1 v1.5 signs to the same bytes as
//! CKM_RSA_PKCS. So, as OpenSC does for cards without PSS, `wrap` encodes
//! the hash for PSS here (EMSA-PSS, RFC 8017 9.1.1: random salt, MGF1) and
//! has the token do only the RSA operation on it. The private key stays in
//! the token.
//!
//! Only the single-part CKM_RSA_PKCS_PSS (what TLS and the signers use) is
//! taken over; every other mechanism goes to the driver as it is.

use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};
use std::collections::HashMap;
use std::ffi::{c_ulong, c_void};
use std::sync::{Mutex, OnceLock};

type CkRv = c_ulong;

const CKR_OK: CkRv = 0;
const CKR_GENERAL_ERROR: CkRv = 0x5;
const CKR_ARGUMENTS_BAD: CkRv = 0x7;
const CKR_DATA_LEN_RANGE: CkRv = 0x21;
const CKR_MECHANISM_PARAM_INVALID: CkRv = 0x71;
const CKR_BUFFER_TOO_SMALL: CkRv = 0x150;

const CKM_RSA_X_509: c_ulong = 0x3;
const CKM_RSA_PKCS_PSS: c_ulong = 0xd;
const CKA_MODULUS: c_ulong = 0x120;

const C_CLOSE_SESSION: usize = 13;
const C_GET_ATTRIBUTE_VALUE: usize = 24;
const C_SIGN_INIT: usize = 42;
const C_SIGN: usize = 43;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    /// From a CKM_SHA* mechanism (CK_RSA_PKCS_PSS_PARAMS.hashAlg).
    fn from_mechanism(m: c_ulong) -> Option<Hash> {
        Some(match m {
            0x220 => Hash::Sha1,
            0x255 => Hash::Sha224,
            0x250 => Hash::Sha256,
            0x260 => Hash::Sha384,
            0x270 => Hash::Sha512,
            _ => return None,
        })
    }

    /// From a CKG_MGF1_* value (CK_RSA_PKCS_PSS_PARAMS.mgf).
    fn from_mgf(g: c_ulong) -> Option<Hash> {
        Some(match g {
            1 => Hash::Sha1,
            2 => Hash::Sha256,
            3 => Hash::Sha384,
            4 => Hash::Sha512,
            5 => Hash::Sha224,
            _ => return None,
        })
    }

    fn len(self) -> usize {
        match self {
            Hash::Sha1 => 20,
            Hash::Sha224 => 28,
            Hash::Sha256 => 32,
            Hash::Sha384 => 48,
            Hash::Sha512 => 64,
        }
    }

    fn of(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut d = D::new();
            for p in parts {
                d.update(p);
            }
            d.finalize().to_vec()
        }
        match self {
            Hash::Sha1 => run::<Sha1>(parts),
            Hash::Sha224 => run::<Sha224>(parts),
            Hash::Sha256 => run::<Sha256>(parts),
            Hash::Sha384 => run::<Sha384>(parts),
            Hash::Sha512 => run::<Sha512>(parts),
        }
    }
}

/// MGF1 (RFC 8017 B.2.1): `len` bytes of mask from `seed`.
fn mgf1(hash: Hash, seed: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + hash.len());
    let mut counter: u32 = 0;
    while out.len() < len {
        out.extend(hash.of(&[seed, &counter.to_be_bytes()]));
        counter += 1;
    }
    out.truncate(len);
    out
}

/// EMSA-PSS-ENCODE (RFC 8017 9.1.1) of the hash `m_hash`, for a modulus of
/// `mod_bits` bits, as a block of the modulus's length in bytes (`k`),
/// ready for the raw RSA operation. None when the sizes do not fit.
pub fn encode(
    m_hash: &[u8],
    hash: Hash,
    mgf: Hash,
    salt: &[u8],
    mod_bits: usize,
) -> Option<Vec<u8>> {
    let h_len = hash.len();
    let em_bits = mod_bits.checked_sub(1)?;
    let em_len = em_bits.div_ceil(8);
    if m_hash.len() != h_len || em_len < h_len + salt.len() + 2 {
        return None;
    }
    let h = hash.of(&[&[0u8; 8], m_hash, salt]);
    let db_len = em_len - h_len - 1;
    let mut db = vec![0u8; db_len - salt.len() - 1];
    db.push(0x01);
    db.extend_from_slice(salt);
    for (b, m) in db.iter_mut().zip(mgf1(mgf, &h, db_len)) {
        *b ^= m;
    }
    db[0] &= 0xffu8 >> (8 * em_len - em_bits);
    let k = mod_bits.div_ceil(8);
    let mut block = vec![0u8; k - em_len];
    block.extend(db);
    block.extend(h);
    block.push(0xbc);
    Some(block)
}

/// A PSS signature under way on a session.
struct Pending {
    hash: Hash,
    mgf: Hash,
    salt_len: usize,
    mod_bits: usize,
}

static ORIGINAL: OnceLock<Vec<usize>> = OnceLock::new();
static PENDING: Mutex<Option<HashMap<c_ulong, Pending>>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<HashMap<c_ulong, Pending>>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

fn original(i: usize) -> usize {
    ORIGINAL.get().expect("wrapped")[i]
}

#[repr(C)]
struct Mechanism {
    mechanism: c_ulong,
    parameter: *const c_void,
    parameter_len: c_ulong,
}

#[repr(C)]
struct PssParams {
    hash_alg: c_ulong,
    mgf: c_ulong,
    s_len: c_ulong,
}

#[repr(C)]
struct Attribute {
    kind: c_ulong,
    value: *mut c_void,
    len: c_ulong,
}

type GetAttributeValue = unsafe extern "C" fn(c_ulong, c_ulong, *mut Attribute, c_ulong) -> CkRv;
type SignInit = unsafe extern "C" fn(c_ulong, *const Mechanism, c_ulong) -> CkRv;
type Sign = unsafe extern "C" fn(c_ulong, *const u8, c_ulong, *mut u8, *mut c_ulong) -> CkRv;
type CloseSession = unsafe extern "C" fn(c_ulong) -> CkRv;

/// The key's modulus size in bits, from its CKA_MODULUS.
///
/// # Safety
///
/// ORIGINAL holds a valid function list; `session` and `key` come from the
/// app's C_SignInit.
unsafe fn modulus_bits(session: c_ulong, key: c_ulong) -> Option<usize> {
    let get: GetAttributeValue = std::mem::transmute(original(C_GET_ATTRIBUTE_VALUE));
    let mut a = Attribute {
        kind: CKA_MODULUS,
        value: std::ptr::null_mut(),
        len: 0,
    };
    if get(session, key, &mut a, 1) != CKR_OK || a.len == 0 || a.len > 1024 {
        return None;
    }
    let mut buf = vec![0u8; a.len as usize];
    a.value = buf.as_mut_ptr() as *mut c_void;
    if get(session, key, &mut a, 1) != CKR_OK {
        return None;
    }
    let n = &buf[..a.len as usize];
    let first = n.iter().position(|b| *b != 0)?;
    Some((n.len() - first) * 8 - n[first].leading_zeros() as usize)
}

/// C_SignInit: CKM_RSA_PKCS_PSS becomes CKM_RSA_X_509 on the driver, and
/// the parameters are kept for C_Sign.
///
/// # Safety
///
/// The arguments of C_SignInit, as PKCS#11 defines them.
unsafe extern "C" fn sign_init(session: c_ulong, m: *const Mechanism, key: c_ulong) -> CkRv {
    let init: SignInit = std::mem::transmute(original(C_SIGN_INIT));
    if let Some(map) = pending().as_mut() {
        map.remove(&session);
    }
    if m.is_null() || (*m).mechanism != CKM_RSA_PKCS_PSS {
        return init(session, m, key);
    }
    let mech = &*m;
    if mech.parameter.is_null() || (mech.parameter_len as usize) < std::mem::size_of::<PssParams>()
    {
        return CKR_MECHANISM_PARAM_INVALID;
    }
    let p = &*(mech.parameter as *const PssParams);
    let (Some(hash), Some(mgf)) = (Hash::from_mechanism(p.hash_alg), Hash::from_mgf(p.mgf)) else {
        return CKR_MECHANISM_PARAM_INVALID;
    };
    let Some(mod_bits) = modulus_bits(session, key) else {
        // Not an RSA key whose modulus can be read: the driver decides.
        return init(session, m, key);
    };
    let raw = Mechanism {
        mechanism: CKM_RSA_X_509,
        parameter: std::ptr::null(),
        parameter_len: 0,
    };
    let rv = init(session, &raw, key);
    if rv == CKR_OK {
        pending().get_or_insert_with(HashMap::new).insert(
            session,
            Pending {
                hash,
                mgf,
                salt_len: p.s_len as usize,
                mod_bits,
            },
        );
    }
    rv
}

extern "C" {
    fn getrandom(buf: *mut c_void, len: usize, flags: u32) -> isize;
}

fn random(len: usize) -> Option<Vec<u8>> {
    let mut b = vec![0u8; len];
    let mut done = 0;
    while done < len {
        // SAFETY: writes at most len - done bytes into b, from done on.
        let n = unsafe { getrandom(b[done..].as_mut_ptr() as *mut c_void, len - done, 0) };
        if n <= 0 {
            return None;
        }
        done += n as usize;
    }
    Some(b)
}

/// C_Sign: for a PSS signature under way, the encoded block is signed raw.
///
/// # Safety
///
/// The arguments of C_Sign, as PKCS#11 defines them.
unsafe extern "C" fn sign(
    session: c_ulong,
    data: *const u8,
    data_len: c_ulong,
    sig: *mut u8,
    sig_len: *mut c_ulong,
) -> CkRv {
    let sign: Sign = std::mem::transmute(original(C_SIGN));
    let Some((hash, mgf, salt_len, mod_bits)) = pending()
        .as_ref()
        .and_then(|m| m.get(&session))
        .map(|p| (p.hash, p.mgf, p.salt_len, p.mod_bits))
    else {
        return sign(session, data, data_len, sig, sig_len);
    };
    if sig_len.is_null() || (data.is_null() && data_len != 0) {
        return CKR_ARGUMENTS_BAD;
    }
    let k = mod_bits.div_ceil(8);
    // Asking for the length, or a buffer too small: the operation goes on.
    if sig.is_null() {
        *sig_len = k as c_ulong;
        return CKR_OK;
    }
    if (*sig_len as usize) < k {
        *sig_len = k as c_ulong;
        return CKR_BUFFER_TOO_SMALL;
    }
    if let Some(map) = pending().as_mut() {
        map.remove(&session);
    }
    let m_hash = std::slice::from_raw_parts(data, data_len as usize);
    let Some(salt) = random(salt_len) else {
        return CKR_GENERAL_ERROR;
    };
    let Some(block) = encode(m_hash, hash, mgf, &salt, mod_bits) else {
        return CKR_DATA_LEN_RANGE;
    };
    sign(
        session,
        block.as_ptr(),
        block.len() as c_ulong,
        sig,
        sig_len,
    )
}

unsafe extern "C" fn close_session(session: c_ulong) -> CkRv {
    if let Some(map) = pending().as_mut() {
        map.remove(&session);
    }
    let close: CloseSession = std::mem::transmute(original(C_CLOSE_SESSION));
    close(session)
}

/// A copy of `module` whose CKM_RSA_PKCS_PSS signatures are made with the
/// driver's raw RSA (see the module's comment).
///
/// # Safety
///
/// As stable::wrap: `module` points to a valid CK_FUNCTION_LIST (2.x, 68
/// functions) or CK_FUNCTION_LIST_3_0 (92), first word the version, the
/// others function pointers. The copy is leaked on purpose, and wrap is
/// called once per process.
pub unsafe fn wrap(module: *mut c_void) -> *mut c_void {
    let version = *(module as *const u8);
    let words = 1 + if version >= 3 { 92 } else { 68 };
    let mut copy = vec![0usize; words];
    std::ptr::copy_nonoverlapping(module as *const usize, copy.as_mut_ptr(), words);
    let _ = ORIGINAL.set(copy[1..69].to_vec());
    for (i, f) in [
        (C_SIGN_INIT, sign_init as *const () as usize),
        (C_SIGN, sign as *const () as usize),
        (C_CLOSE_SESSION, close_session as *const () as usize),
    ] {
        if copy[1 + i] != 0 {
            copy[1 + i] = f;
        }
    }
    eprintln!("tessera: RSA-PSS signatures made with the driver's raw RSA");
    Box::leak(copy.into_boxed_slice()).as_mut_ptr() as *mut c_void
}

/// Whether the driver's environment asks for it.
pub fn wanted() -> bool {
    std::env::var("TESSERA_PSS_VIA_RAW").as_deref() == Ok("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 8017's MGF1 against a known answer: MGF1-SHA1("foo", 3) is
    /// 1a c9 07 (the example in many PKCS#1 test suites).
    #[test]
    fn mgf1_known_answer() {
        assert_eq!(mgf1(Hash::Sha1, b"foo", 3), vec![0x1a, 0xc9, 0x07]);
        assert_eq!(
            mgf1(Hash::Sha1, b"bar", 5),
            vec![0xbc, 0x0c, 0x65, 0x5e, 0x01]
        );
    }

    /// The block's structure: as long as the modulus, ends in 0xbc, top bit
    /// clear for a 2048-bit key; and decoding it gives back the salt.
    #[test]
    fn encode_has_the_pss_structure() {
        let m_hash = Hash::Sha256.of(&[b"x"]);
        let salt = [7u8; 32];
        let b = encode(&m_hash, Hash::Sha256, Hash::Sha256, &salt, 2048).unwrap();
        assert_eq!(b.len(), 256);
        assert_eq!(*b.last().unwrap(), 0xbc);
        assert_eq!(b[0] & 0x80, 0);
        let (masked, h) = (&b[..256 - 33], &b[256 - 33..256 - 1]);
        let mut db = masked.to_vec();
        for (x, m) in db.iter_mut().zip(mgf1(Hash::Sha256, h, masked.len())) {
            *x ^= m;
        }
        db[0] &= 0x7f;
        assert_eq!(&db[db.len() - 32..], &salt);
        assert_eq!(db[db.len() - 33], 0x01);
        assert!(db[..db.len() - 33].iter().all(|x| *x == 0));
        assert_eq!(h, Hash::Sha256.of(&[&[0u8; 8], &m_hash, &salt]));
        // A wrong hash length, or a key too small, gives nothing.
        assert!(encode(&m_hash[..31], Hash::Sha256, Hash::Sha256, &salt, 2048).is_none());
        assert!(encode(&m_hash, Hash::Sha512, Hash::Sha512, &[0; 64], 512).is_none());
    }
}
