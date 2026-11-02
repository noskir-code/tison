//! Two-factor authentication codes (TOTP, RFC 6238): the 6 digits of an
//! authenticator app (Aegis, Google Authenticator, 2FAS, Bitwarden...)
//! changing every 30 seconds, from a secret shared once with a QR code
//! (see `qr`). SHA-1, HMAC and base32 are written here, as the apps use them.

/// SHA-1 (FIPS 180-4): what TOTP apps use by default. Fine for HMAC.
fn sha1(message: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut data = message.to_vec();
    let bit_length = (message.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend(bit_length.to_be_bytes());
    for block in data.chunks(64) {
        let mut words = [0u32; 80];
        for (index, word) in block.chunks(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (mixed, constant) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a.rotate_left(5).wrapping_add(mixed).wrapping_add(e).wrapping_add(constant).wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (value, added) in state.iter_mut().zip([a, b, c, d, e]) {
            *value = value.wrapping_add(added);
        }
    }
    let mut digest = [0u8; 20];
    for (index, value) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    digest
}

/// HMAC-SHA1 (RFC 2104).
fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..20].copy_from_slice(&sha1(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner: Vec<u8> = block.iter().map(|byte| byte ^ 0x36).chain(message.iter().copied()).collect();
    let outer: Vec<u8> = block.iter().map(|byte| byte ^ 0x5C).chain(sha1(&inner)).collect();
    sha1(&outer)
}

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 without padding (RFC 4648): how the secret is shown and put in
/// the QR code.
pub fn base32_encode(data: &[u8]) -> String {
    let mut text = String::with_capacity(data.len() * 8 / 5 + 1);
    let (mut buffer, mut bits) = (0u32, 0);
    for byte in data {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            text.push(BASE32[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        text.push(BASE32[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    text
}

/// The reverse, ignoring spaces, dashes, padding and case.
pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut data = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u32, 0);
    for letter in text.chars().filter(|letter| !matches!(letter, ' ' | '-' | '=')) {
        let value = BASE32.iter().position(|known| *known as char == letter.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            data.push((buffer >> bits) as u8);
        }
    }
    Some(data)
}

/// Seconds a code lasts.
pub const PERIOD: u64 = 30;
const DIGITS: u32 = 6;

/// The code of time step `step` (seconds since 1970 / 30).
pub fn code(secret: &[u8], step: u64) -> u32 {
    let mac = hmac_sha1(secret, &step.to_be_bytes());
    // Dynamic truncation (RFC 4226).
    let offset = usize::from(mac[19] & 0x0F);
    let number = u32::from_be_bytes([mac[offset] & 0x7F, mac[offset + 1], mac[offset + 2], mac[offset + 3]]);
    number % 10u32.pow(DIGITS)
}

pub fn current_step() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() / PERIOD)
        .unwrap_or(0)
}

/// The step of `typed` if it's the code now, 30 seconds before or after
/// (phones' clocks drift), and after `last_used`: a code works once.
pub fn verify(secret: &[u8], typed: &str, now: u64, last_used: Option<u64>) -> Option<u64> {
    let digits: String = typed.chars().filter(|letter| !letter.is_whitespace()).collect();
    if digits.len() != DIGITS as usize || !digits.chars().all(|letter| letter.is_ascii_digit()) {
        return None;
    }
    let typed: u32 = digits.parse().ok()?;
    [now.saturating_sub(1), now, now + 1]
        .into_iter()
        .filter(|step| last_used.is_none_or(|last| *step > last))
        .find(|step| code(secret, *step) == typed)
}

/// A new secret: 160 random bits, as RFC 4226 advises.
pub fn new_secret() -> Vec<u8> {
    use rsa::rand_core::{OsRng, RngCore};
    let mut secret = vec![0u8; 20];
    OsRng.fill_bytes(&mut secret);
    secret
}

/// What the QR code holds, read by every authenticator app:
/// `otpauth://totp/Issuer:account?secret=...&issuer=Issuer`.
pub fn setup_url(issuer: &str, account: &str, secret: &[u8]) -> String {
    let issuer = crate::entities::event::url_encode(issuer).replace('+', "%20");
    let account = crate::entities::event::url_encode(account).replace('+', "%20");
    format!("otpauth://totp/{issuer}:{account}?secret={}&issuer={issuer}", base32_encode(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn sha1_matches_the_standard() {
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn hmac_matches_rfc_2202() {
        assert_eq!(hex(&hmac_sha1(&[0x0b; 20], b"Hi There")), "b617318655057264e28bc0b6fb378c8ef146be00");
        assert_eq!(hex(&hmac_sha1(b"Jefe", b"what do ya want for nothing?")), "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
        assert_eq!(hex(&hmac_sha1(&[0xaa; 80], b"Test Using Larger Than Block-Size Key - Hash Key First")), "aa4ae5e15272d00e95705637ce8a3b55ed402112");
    }

    #[test]
    fn codes_match_rfc_6238() {
        // Its test secret, the SHA-1 column, 8 digits cut to the last 6.
        let secret = b"12345678901234567890";
        for (seconds, expected) in [(59u64, 94287082u32), (1111111109, 7081804), (1111111111, 14050471), (1234567890, 89005924), (2000000000, 69279037)] {
            assert_eq!(code(secret, seconds / PERIOD), expected % 1_000_000, "at {seconds}");
        }
    }

    #[test]
    fn codes_are_checked_once_and_near_now() {
        let secret = b"12345678901234567890";
        let now = 1111111111 / PERIOD;
        let typed = format!("{:06}", code(secret, now));
        assert_eq!(verify(secret, &typed, now, None), Some(now));
        // Already used: refused.
        assert_eq!(verify(secret, &typed, now, Some(now)), None);
        // A step late is fine, two aren't.
        assert_eq!(verify(secret, &typed, now + 1, None), Some(now));
        assert_eq!(verify(secret, &typed, now + 2, None), None);
        assert_eq!(verify(secret, "12345", now, None), None);
        assert_eq!(verify(secret, &format!("{} {}", &typed[..3], &typed[3..]), now, None), Some(now));
    }

    #[test]
    fn base32_round_trip() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("mzxw 6ytb-oi======"), Some(b"foobar".to_vec()));
        assert_eq!(base32_decode("1"), None);
        let secret = new_secret();
        assert_eq!(base32_decode(&base32_encode(&secret)), Some(secret));
    }
}
