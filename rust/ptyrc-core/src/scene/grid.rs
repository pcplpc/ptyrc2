// SPDX-License-Identifier: GPL-3.0-or-later
//! `content.grid`: rows of 16-byte cells, base64. The cell is that of a
//! canvas of `ptyrc-box` (ABI 1): scalar `u32`, `fg`, `bg`, attributes
//! `u16`; little-endian. Bytes 14..16, a `u16`: the cell is repeated
//! that many times more.

use super::spec::SceneError;

/// Bytes of a cell.
pub const CELL: usize = 16;
/// Widest row, in cells, repeats counted.
pub const MAX_WIDTH: usize = 4096;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64, standard alphabet, padded.
pub fn encode(bytes: &[u8], out: &mut String) {
    out.reserve(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]);
        for shift in [18, 12, 6, 0] {
            out.push(ALPHABET[(n >> shift) as usize & 63] as char);
        }
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let b1 = rest.get(1).copied().unwrap_or(0);
        let n = u32::from(rest[0]) << 16 | u32::from(b1) << 8;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if rest.len() == 2 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push('=');
    }
}

const fn table() -> [u8; 256] {
    let mut t = [0xffu8; 256];
    let mut i = 0;
    while i < 64 {
        t[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    t
}

const DECODE: [u8; 256] = table();

fn bad(what: &str) -> SceneError {
    SceneError::BadParams(format!("content: grid: {what}"))
}

/// One row: base64 of whole cells. Padding required.
pub fn decode_row(text: &str, out: &mut Vec<u8>) -> Result<(), SceneError> {
    let b = text.as_bytes();
    if !b.len().is_multiple_of(4) {
        return Err(bad("not base64"));
    }
    let pad = match b {
        [.., b'=', b'='] => 2,
        [.., b'='] => 1,
        _ => 0,
    };
    let quads = b.len() / 4;
    let whole = if pad > 0 { quads - 1 } else { quads };
    out.clear();
    out.resize(whole * 3, 0);
    let mut invalid = 0u8;
    for (q, o) in b.chunks_exact(4).zip(out.chunks_exact_mut(3)) {
        let v = [
            DECODE[q[0] as usize],
            DECODE[q[1] as usize],
            DECODE[q[2] as usize],
            DECODE[q[3] as usize],
        ];
        invalid |= v[0] | v[1] | v[2] | v[3];
        let n =
            u32::from(v[0]) << 18 | u32::from(v[1]) << 12 | u32::from(v[2]) << 6 | u32::from(v[3]);
        o.copy_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    if pad > 0 {
        let q = &b[b.len() - 4..];
        let mut n = 0u32;
        for &c in &q[..4 - pad] {
            let v = DECODE[c as usize];
            invalid |= v;
            n = n << 6 | u32::from(v & 63);
        }
        n <<= 6 * pad as u32;
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
    }
    // A valid digit is under 64; `DECODE` marks the others 0xff.
    if invalid & 0xc0 != 0 {
        return Err(bad("not base64"));
    }
    if !out.len().is_multiple_of(CELL) {
        return Err(bad("a row is not whole cells of 16 bytes"));
    }
    expand(out)
}

/// Repeats spelled out; bytes 14..16 of each cell then 0.
fn expand(row: &mut Vec<u8>) -> Result<(), SceneError> {
    let count = |c: &[u8]| usize::from(u16::from_le_bytes([c[14], c[15]]));
    let cells: usize = row.chunks_exact(CELL).map(|c| 1 + count(c)).sum();
    if cells > MAX_WIDTH {
        return Err(bad(&format!("a row wider than {MAX_WIDTH} cells")));
    }
    if cells == row.len() / CELL {
        return Ok(());
    }
    // In place, from the end: a cell is read before its place is written.
    let sent = row.len() / CELL;
    row.resize(cells * CELL, 0);
    let mut end = cells;
    for i in (0..sent).rev() {
        let mut c = [0u8; CELL];
        c.copy_from_slice(&row[i * CELL..(i + 1) * CELL]);
        let n = 1 + count(&c);
        c[14..].fill(0);
        for at in end - n..end {
            row[at * CELL..(at + 1) * CELL].copy_from_slice(&c);
        }
        end -= n;
    }
    Ok(())
}

/// A row of a canvas as `grid` sends it: runs of equal cells as one
/// cell and its repeats; trailing holes left out when `trim`.
pub fn encode_row(cells: &[u8], trim: bool, out: &mut String) {
    let mut packed: Vec<u8> = Vec::with_capacity(cells.len());
    let mut last: Option<&[u8]> = None;
    let mut repeats = 0u16;
    let flush = |cell: &[u8], repeats: u16, packed: &mut Vec<u8>| {
        packed.extend_from_slice(&cell[..14]);
        packed.extend_from_slice(&repeats.to_le_bytes());
    };
    for c in cells.chunks_exact(CELL) {
        match last {
            Some(l) if l[..14] == c[..14] && repeats < u16::MAX => repeats += 1,
            Some(l) => {
                flush(l, repeats, &mut packed);
                (last, repeats) = (Some(c), 0);
            }
            None => last = Some(c),
        }
    }
    if let Some(l) = last
        && !(trim && l[..4] == [0; 4] && l[4..14] == [0; 10])
    {
        flush(l, repeats, &mut packed);
    }
    encode(&packed, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for n in 0..40 {
            // Bytes 14..16 of a cell are a count: 0 here.
            let bytes: Vec<u8> = (0..n)
                .map(|i| {
                    if i % CELL >= 14 {
                        0
                    } else {
                        (i * 37 + 11) as u8
                    }
                })
                .collect();
            let mut s = String::new();
            encode(&bytes, &mut s);
            let mut back = Vec::new();
            let ok = decode_row(&s, &mut back);
            if n % CELL == 0 {
                ok.unwrap();
                assert_eq!(back, bytes);
            } else {
                assert!(ok.is_err());
            }
        }
        let mut s = String::new();
        encode(b"hello", &mut s);
        assert_eq!(s, "aGVsbG8=");
    }

    #[test]
    fn repeats() {
        let cell = |v: u8, rep: u16| {
            let mut c = [0u8; CELL];
            c[0] = v;
            c[4] = 3;
            c[14..16].copy_from_slice(&rep.to_le_bytes());
            c
        };
        let plain: Vec<u8> = [cell(b'a', 0), cell(b'a', 0), cell(b'a', 0), cell(b'b', 0)]
            .concat()
            .into_iter()
            .chain([0u8; 2 * CELL])
            .collect();
        let mut s = String::new();
        encode_row(&plain, true, &mut s);
        let mut sent = String::new();
        encode(&[cell(b'a', 2), cell(b'b', 0)].concat(), &mut sent);
        assert_eq!(s, sent);
        let mut back = Vec::new();
        decode_row(&s, &mut back).unwrap();
        assert_eq!(back, plain[..4 * CELL]);
        s.clear();
        encode_row(&plain, false, &mut s);
        decode_row(&s, &mut back).unwrap();
        assert_eq!(back, plain);
        // Garbage in the guest's padding is not a count.
        let mut odd = plain.clone();
        odd[14] = 9;
        s.clear();
        encode_row(&odd, false, &mut s);
        decode_row(&s, &mut back).unwrap();
        assert_eq!(back, plain);
        s.clear();
        encode(&cell(b'a', 4096), &mut s);
        assert!(decode_row(&s, &mut back).is_err());
    }

    #[test]
    fn refuses() {
        let mut out = Vec::new();
        for bad in ["A", "AA=A", "AAA=AAAA", "@AAA", "AA==AAAA", "AAA\n"] {
            assert!(decode_row(bad, &mut out).is_err(), "{bad}");
        }
    }
}
