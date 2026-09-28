//! Heap: an upward bump allocator over up to 256 KiB (I-50), with a
//! high-water mark.
//!
//! Without a `RequestHeapFrame` the runtime maps 32 KiB at the heap start;
//! the allocator hands out up to 256 KiB and an access past the mapped
//! region faults cleanly (the transaction fails, nothing is written). The
//! keeper's retry ladder (§8.2) re-sends a write that faulted with a
//! 256-KiB heap frame, so a fill worse than the worst gated one costs a
//! retry, never a frozen province. Every gated fill keeps the peak ≤ 28 KiB
//! (`frontier_abi::budgets::HEAP_GATE`).
//!
//! The arithmetic is [`Bump`], a plain struct tested on the host; the SBF
//! global allocator is `Bump` over the program heap region (feature
//! `custom-heap`, on by default). Only the top block is ever freed.

use core::alloc::Layout;

/// The largest heap a transaction can request (256 KiB).
pub const HEAP_MAX: usize = frontier_abi::budgets::HEAP_FRAME as usize;
/// The heap the runtime maps without a heap frame.
pub const HEAP_DEFAULT: usize = 32 * 1024;
/// SBF program heap start (`MM_HEAP_START`).
pub const HEAP_START: usize = 0x3_0000_0000;

const WORD: usize = core::mem::size_of::<usize>();

/// A bump allocator over `[start, start + len)`: the first word holds the
/// next free address (0 before the first allocation), the second the
/// highest end ever handed out.
pub struct Bump {
    pub start: usize,
    pub len: usize,
}

impl Bump {
    /// Header words (next, peak).
    pub const HEAD: usize = 2 * WORD;

    /// # Safety
    /// `[start, start + HEAD)` must be writable and exclusively owned by
    /// this allocator.
    pub unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let next = self.start as *mut usize;
        let peak = (self.start + WORD) as *mut usize;
        let mut cur = *next;
        if cur == 0 {
            cur = self.start + Self::HEAD;
        }
        let align = l.align().max(1);
        let Some(at) = cur.checked_add(align - 1).map(|x| x & !(align - 1)) else {
            return core::ptr::null_mut();
        };
        let end = match at.checked_add(l.size()) {
            Some(e) if e <= self.start + self.len => e,
            _ => return core::ptr::null_mut(),
        };
        *next = end;
        if end > *peak {
            *peak = end;
        }
        at as *mut u8
    }

    /// Frees `p` if it is the top block (otherwise a no-op).
    ///
    /// # Safety
    /// As [`Bump::alloc`]; `p` came from this allocator with layout `l`.
    pub unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let next = self.start as *mut usize;
        if (p as usize).checked_add(l.size()) == Some(*next) {
            *next = p as usize;
        }
    }

    /// Bytes ever handed out, header included (0 before any allocation).
    ///
    /// # Safety
    /// As [`Bump::alloc`].
    pub unsafe fn peak(&self) -> usize {
        let p = *((self.start + WORD) as *const usize);
        if p == 0 {
            0
        } else {
            p - self.start
        }
    }
}

#[cfg(all(target_os = "solana", feature = "custom-heap"))]
mod imp {
    use core::alloc::{GlobalAlloc, Layout};

    pub struct Global;

    const BUMP: super::Bump = super::Bump {
        start: super::HEAP_START,
        len: super::HEAP_MAX,
    };

    // SAFETY: the program heap region belongs to this allocator alone; the
    // runtime zeroes it before the instruction, so `next` starts at 0.
    unsafe impl GlobalAlloc for Global {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            BUMP.alloc(l)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            BUMP.dealloc(p, l)
        }
    }

    #[global_allocator]
    static A: Global = Global;

    pub fn peak() -> u64 {
        // SAFETY: reads the allocator's own header word.
        unsafe { BUMP.peak() as u64 }
    }
}

/// Heap bytes ever handed out in this instruction, header included (0 off
/// chain or with the default allocator).
pub fn peak() -> u64 {
    #[cfg(all(target_os = "solana", feature = "custom-heap"))]
    {
        imp::peak()
    }
    #[cfg(not(all(target_os = "solana", feature = "custom-heap")))]
    {
        0
    }
}

/// Runs `f` and then frees every heap block it allocated: the bump
/// allocator's `next` word is restored, so the space is handed out again
/// (the high-water mark is kept). Sound only when nothing `f` allocated is
/// live afterwards: `f` must return plain data (W4-A D9; SkipQuiet's
/// kernel quiet test).
pub fn scoped<T: Copy>(f: impl FnOnce() -> T) -> T {
    #[cfg(all(target_os = "solana", feature = "custom-heap"))]
    {
        let next = HEAP_START as *mut usize;
        // SAFETY: the first word of the heap region is the bump allocator's
        // `next` pointer ([`Bump`]); it is read before and restored after
        // `f`, whose allocations are all dead when it returns (`T: Copy`
        // owns no heap block).
        let saved = unsafe { *next };
        let v = f();
        // SAFETY: as above.
        unsafe { *next = saved };
        v
    }
    #[cfg(not(all(target_os = "solana", feature = "custom-heap")))]
    {
        f()
    }
}

/// CU checkpoint of `trace` builds: logs `("CU", tag, heap peak)` and the
/// remaining compute units; the harness takes differences. Free in every
/// other build.
#[inline(always)]
pub fn trace_checkpoint(_tag: u64) {
    #[cfg(all(feature = "trace", target_os = "solana"))]
    {
        solana_program::log::sol_log_64(0x4355, _tag, peak(), 0, 0);
        solana_program::log::sol_log_compute_units();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_allocates_upward_frees_the_top_and_tracks_the_peak() {
        let mut region = alloc::vec![0usize; 1024];
        let start = region.as_mut_ptr() as usize;
        let b = Bump {
            start,
            len: 1024 * WORD,
        };
        unsafe {
            assert_eq!(b.peak(), 0);
            let a = b.alloc(Layout::from_size_align(10, 1).unwrap());
            assert_eq!(a as usize, start + Bump::HEAD);
            let c = b.alloc(Layout::from_size_align(8, 8).unwrap());
            assert_eq!(c as usize % 8, 0);
            assert!(c as usize >= a as usize + 10);
            let peak = b.peak();
            b.dealloc(c, Layout::from_size_align(8, 8).unwrap());
            // freed the top: the next allocation reuses it
            let d = b.alloc(Layout::from_size_align(8, 8).unwrap());
            assert_eq!(d, c);
            assert_eq!(b.peak(), peak);
            // not the top: no-op
            b.dealloc(a, Layout::from_size_align(10, 1).unwrap());
            let e = b.alloc(Layout::from_size_align(1, 1).unwrap());
            assert!(e as usize > d as usize);
            // past the region: null, state unchanged
            let big = b.alloc(Layout::from_size_align(1024 * WORD, 1).unwrap());
            assert!(big.is_null());
            let f = b.alloc(Layout::from_size_align(1, 1).unwrap());
            assert_eq!(f as usize, e as usize + 1);
        }
        drop(region);
    }

    #[test]
    fn scoped_returns_the_closures_value() {
        assert_eq!(scoped(|| 41 + 1), 42);
        assert_eq!(scoped(|| Ok::<bool, ()>(true)), Ok(true));
    }

    #[test]
    fn constants_follow_the_contract() {
        assert_eq!(HEAP_MAX, 262_144);
        assert!(frontier_abi::budgets::HEAP_GATE as usize <= HEAP_DEFAULT);
        assert_eq!(peak(), 0);
    }
}
