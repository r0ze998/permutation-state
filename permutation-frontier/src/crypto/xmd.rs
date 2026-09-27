//! `expand_message_xmd` (RFC 9380 §5.3.1) over SHA-256 (SP-V2, hashing
//! through `permutation_rules::hash::sha256`, the `sol_sha256` syscall on
//! chain).

pub use permutation_rules::hash::sha256;

/// Fills `out` (≤ 255 × 32 bytes) with `expand_message_xmd(msg, dst)`;
/// `dst` ≤ 255 bytes. Longer requests leave `out` zeroed past 8,160 B.
pub fn xmd_sha256(msg: &[u8], dst: &[u8], out: &mut [u8]) {
    let zpad = [0u8; 64];
    let len = out.len();
    let ell = len.div_ceil(32).min(255);
    let dlen = [dst.len() as u8];
    let lib = [(len >> 8) as u8, len as u8];
    let b0 = sha256(&[&zpad, msg, &lib, &[0u8], dst, &dlen]);
    let mut bi = sha256(&[&b0, &[1u8], dst, &dlen]);
    for i in 1..=ell {
        let at = (i - 1) * 32;
        let take = core::cmp::min(32, len - at);
        out[at..at + take].copy_from_slice(&bi[..take]);
        if i < ell {
            let mut t = [0u8; 32];
            for (j, v) in t.iter_mut().enumerate() {
                *v = b0[j] ^ bi[j];
            }
            bi = sha256(&[&t, &[(i + 1) as u8], dst, &dlen]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> alloc::vec::Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 9380 Appendix K.1 (expand_message_xmd, SHA-256), msg "" and
    /// "abc", len_in_bytes 0x20 and 0x80.
    #[test]
    fn rfc9380_vectors() {
        let dst = b"QUUX-V01-CS02-with-expander-SHA256-128";
        let mut o = [0u8; 32];
        xmd_sha256(b"", dst, &mut o);
        assert_eq!(
            o.to_vec(),
            hex("68a985b87eb6b46952128911f2a4412bbc302a9d759667f87f7a21d803f07235")
        );
        xmd_sha256(b"abc", dst, &mut o);
        assert_eq!(
            o.to_vec(),
            hex("d8ccab23b5985ccea865c6c97b6e5b8350e794e603b4b97902f53a8a0d605615")
        );
        let mut o = [0u8; 128];
        xmd_sha256(b"", dst, &mut o);
        assert_eq!(
            o.to_vec(),
            hex("af84c27ccfd45d41914fdff5df25293e221afc53d8ad2ac06d5e3e29485dadbee0d121587713a3e0dd4d5e69e93eb7cd4f5df4cd103e188cf60cb02edc3edf18eda8576c412b18ffb658e3dd6ec849469b979d444cf7b26911a08e63cf31f9dcc541708d3491184472c2c29bb749d4286b004ceb5ee6b9a7fa5b646c993f0ced")
        );
    }
}
