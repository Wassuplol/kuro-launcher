//! Sealing a session cookie with the operating system.
//!
//! DPAPI binds the ciphertext to the Windows user who wrote it. Copy the file to
//! another account or another machine and it decrypts to nothing, which is the
//! property that makes storing a cookie at rest acceptable at all.
//!
//! Nothing here ever writes a plaintext cookie to disk. The sealed bytes go out
//! of this module only to a file, and come back only to be unsealed for a
//! request to Roblox.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use crate::error::{FailureCode, LaunchError};

fn blob(data: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_mut_ptr(),
    }
}

/// Encrypts for the current Windows user.
pub fn seal(plain: &str) -> Result<Vec<u8>, LaunchError> {
    let mut input = plain.as_bytes().to_vec();
    let mut out = CRYPT_INTEGER_BLOB::default();

    // SAFETY: `input` outlives the call and `out` is written by the API, then
    // freed with the allocator the API used.
    unsafe {
        let source = blob(&mut input);
        CryptProtectData(
            &source,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Windows could not encrypt the session: {e}"),
            )
        })?;

        let sealed = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
        Ok(sealed)
    }
}

/// Decrypts for the current Windows user. A blob from another user fails here,
/// which is the whole point: it is reported as a session that cannot be read
/// rather than as a mystery.
pub fn unseal(sealed: &[u8]) -> Result<String, LaunchError> {
    let mut input = sealed.to_vec();
    let mut out = CRYPT_INTEGER_BLOB::default();

    // SAFETY: same contract as `seal`; the output buffer is freed below.
    unsafe {
        let source = blob(&mut input);
        CryptUnprotectData(
            &source,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| {
            LaunchError::new(
                FailureCode::SessionExpired,
                format!("This session was saved for another Windows user: {e}"),
            )
            .with_action("Add the account again")
        })?;

        let plain = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));

        String::from_utf8(plain).map_err(|_| {
            LaunchError::new(
                FailureCode::SessionExpired,
                "The saved session is not readable text.",
            )
            .with_action("Add the account again")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole design rests on: what lands on disk is not the
    /// cookie, and only this user can get the cookie back out.
    #[test]
    fn a_sealed_cookie_is_not_the_cookie() {
        let cookie = "0123456789ABCDEF-test-cookie-value";
        let sealed = seal(cookie).expect("seal");

        assert_ne!(sealed, cookie.as_bytes());
        assert!(
            !sealed
                .windows(cookie.len())
                .any(|window| window == cookie.as_bytes()),
            "the plaintext must not appear anywhere in the ciphertext"
        );
        // DPAPI blobs carry this header, which is how a sealed value is
        // recognisable on disk later.
        assert_eq!(&sealed[..4], &[0x01, 0x00, 0x00, 0x00]);

        assert_eq!(unseal(&sealed).expect("unseal"), cookie);
    }

    /// A blob that is not ours is refused, not guessed at.
    #[test]
    fn a_foreign_blob_is_refused() {
        let junk = vec![0u8; 32];
        assert!(unseal(&junk).is_err());
    }
}
