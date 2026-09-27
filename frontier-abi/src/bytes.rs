//! Little-endian field access over byte slices.
//!
//! Every accessor is bounds-checked and returns `None` (readers) or
//! `false`/`Err` (writers) instead of panicking, so the program can use them
//! under its no-`unwrap` rule (§3.3). Callers normally check an account's
//! length once against its layout `SIZE`; after that every fixed offset of
//! the layout is in range.

macro_rules! rw {
    ($rd:ident, $wr:ident, $t:ty, $n:expr) => {
        #[inline]
        pub fn $rd(d: &[u8], off: usize) -> Option<$t> {
            let b = d.get(off..off.checked_add($n)?)?;
            let mut a = [0u8; $n];
            a.copy_from_slice(b);
            Some(<$t>::from_le_bytes(a))
        }
        #[inline]
        pub fn $wr(d: &mut [u8], off: usize, v: $t) -> bool {
            match off.checked_add($n).and_then(|end| d.get_mut(off..end)) {
                Some(b) => {
                    b.copy_from_slice(&v.to_le_bytes());
                    true
                }
                None => false,
            }
        }
    };
}

rw!(rd_u16, wr_u16, u16, 2);
rw!(rd_u32, wr_u32, u32, 4);
rw!(rd_u64, wr_u64, u64, 8);
rw!(rd_i16, wr_i16, i16, 2);
rw!(rd_i32, wr_i32, i32, 4);
rw!(rd_i64, wr_i64, i64, 8);

#[inline]
pub fn rd_u8(d: &[u8], off: usize) -> Option<u8> {
    d.get(off).copied()
}

#[inline]
pub fn wr_u8(d: &mut [u8], off: usize, v: u8) -> bool {
    match d.get_mut(off) {
        Some(b) => {
            *b = v;
            true
        }
        None => false,
    }
}

/// `N` bytes at `off`.
#[inline]
pub fn rd_arr<const N: usize>(d: &[u8], off: usize) -> Option<[u8; N]> {
    let b = d.get(off..off.checked_add(N)?)?;
    let mut a = [0u8; N];
    a.copy_from_slice(b);
    Some(a)
}

/// Borrow `N` bytes at `off` without copying.
#[inline]
pub fn rd_ref<const N: usize>(d: &[u8], off: usize) -> Option<&[u8; N]> {
    d.get(off..off.checked_add(N)?)?.try_into().ok()
}

#[inline]
pub fn wr_arr(d: &mut [u8], off: usize, v: &[u8]) -> bool {
    match off.checked_add(v.len()).and_then(|end| d.get_mut(off..end)) {
        Some(b) => {
            b.copy_from_slice(v);
            true
        }
        None => false,
    }
}

/// A forward reader over instruction data or a log body.
#[derive(Clone, Copy, Debug)]
pub struct Cursor<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub const fn new(d: &'a [u8]) -> Self {
        Cursor { d, pos: 0 }
    }
    pub const fn pos(&self) -> usize {
        self.pos
    }
    pub fn remaining(&self) -> usize {
        self.d.len().saturating_sub(self.pos)
    }
    pub fn rest(&self) -> &'a [u8] {
        self.d.get(self.pos..).unwrap_or(&[])
    }
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.d.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    pub fn arr<const N: usize>(&mut self) -> Option<[u8; N]> {
        let s = self.take(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(s);
        Some(a)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.arr().map(u16::from_le_bytes)
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.arr().map(u32::from_le_bytes)
    }
    pub fn u64(&mut self) -> Option<u64> {
        self.arr().map(u64::from_le_bytes)
    }
    pub fn i16(&mut self) -> Option<i16> {
        self.arr().map(i16::from_le_bytes)
    }
    pub fn i32(&mut self) -> Option<i32> {
        self.arr().map(i32::from_le_bytes)
    }
    pub fn i64(&mut self) -> Option<i64> {
        self.arr().map(i64::from_le_bytes)
    }
    /// All input consumed.
    pub fn done(&self) -> bool {
        self.pos == self.d.len()
    }
}

/// A forward writer into a caller-provided buffer.
#[derive(Debug)]
pub struct Writer<'a> {
    d: &'a mut [u8],
    pos: usize,
    ok: bool,
}

impl<'a> Writer<'a> {
    pub fn new(d: &'a mut [u8]) -> Self {
        Writer {
            d,
            pos: 0,
            ok: true,
        }
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        if self.ok {
            self.ok = wr_arr(self.d, self.pos, v);
            self.pos += v.len();
        }
        self
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.bytes(&[v])
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    /// Bytes written, or `None` if the buffer was too small.
    pub fn finish(&self) -> Option<usize> {
        self.ok.then_some(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_is_none_not_panic() {
        let mut d = [0u8; 8];
        assert!(wr_u64(&mut d, 0, u64::MAX));
        assert_eq!(rd_u64(&d, 0), Some(u64::MAX));
        assert_eq!(rd_u64(&d, 1), None);
        assert_eq!(rd_u32(&d, usize::MAX), None);
        assert!(!wr_u32(&mut d, 6, 1));
        assert!(!wr_u32(&mut d, usize::MAX - 1, 1));
        assert_eq!(rd_arr::<9>(&d, 0), None);
        let mut w = Writer::new(&mut d);
        w.u64(1).u8(2);
        assert_eq!(w.finish(), None);
        let mut c = Cursor::new(&[1, 2, 3]);
        assert_eq!(c.u16(), Some(0x0201));
        assert_eq!(c.u16(), None);
        assert_eq!(c.u8(), Some(3));
        assert!(c.done());
    }
}
