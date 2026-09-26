//! Heap: an upward-growing bump allocator over up to 256 KiB.
//!
//! The runtime's default allocator starts at the *top* of a fixed 32 KiB
//! region. Decoding the world and running a tick needs far more, so heavy
//! instructions request a 256 KiB heap frame
//! (`ComputeBudgetInstruction::request_heap_frame(HEAP_BYTES)`). Growing
//! upward from the bottom means light instructions (entry, orders, claims)
//! still work without that request as long as they stay under 32 KiB. The
//! last allocation is resized in place, which keeps growing `Vec`s from
//! leaving copies behind.
//!
//! The arithmetic is `Bump`, a plain struct tested on the host; the SBF
//! allocator is `Bump` over the program's heap region.

use core::alloc::Layout;

pub const HEAP_BYTES: usize = 256 * 1024;

/// The allocator over `[start, end)`: the first word at `start` holds the
/// next free address (0 = unused). Plain arithmetic, tested on the host.
pub struct Bump {
    pub start: usize,
    pub end: usize,
}

impl Bump {
    const HEAD: usize = core::mem::size_of::<usize>();

    #[inline]
    unsafe fn next(&self) -> *mut usize {
        self.start as *mut usize
    }

    #[inline]
    fn align_up(p: usize, align: usize) -> usize {
        (p + align - 1) & !(align - 1)
    }

    /// # Safety
    /// `[start, end)` is writable memory owned by this allocator, and
    /// `start` is aligned for `usize`.
    pub unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut pos = *self.next();
        if pos == 0 {
            pos = self.start + Self::HEAD;
        }
        let at = Self::align_up(pos, layout.align());
        let end = match at.checked_add(layout.size()) {
            Some(e) if e <= self.end => e,
            _ => return core::ptr::null_mut(),
        };
        *self.next() = end;
        at as *mut u8
    }

    /// # Safety
    /// As `alloc`; `ptr` and `layout` describe a block this allocator returned.
    pub unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Give back the most recent allocation; bump otherwise.
        if ptr as usize + layout.size() == *self.next() {
            *self.next() = ptr as usize;
        }
    }

    /// # Safety
    /// As `dealloc`.
    pub unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = ptr as usize;
        if p + layout.size() == *self.next() {
            // The last block: grow or shrink in place.
            return match p.checked_add(new_size) {
                Some(e) if e <= self.end => {
                    *self.next() = e;
                    ptr
                }
                _ => core::ptr::null_mut(),
            };
        }
        if new_size <= layout.size() {
            return ptr;
        }
        let fresh = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
        if !fresh.is_null() {
            core::ptr::copy_nonoverlapping(ptr, fresh, layout.size());
        }
        fresh
    }
}

#[cfg(all(feature = "custom-heap", target_os = "solana"))]
mod upward {
    use super::{Bump, HEAP_BYTES};
    use core::alloc::{GlobalAlloc, Layout};
    use solana_program::entrypoint::HEAP_START_ADDRESS;

    const B: Bump = Bump {
        start: HEAP_START_ADDRESS as usize,
        end: HEAP_START_ADDRESS as usize + HEAP_BYTES,
    };

    pub struct Upward;

    unsafe impl GlobalAlloc for Upward {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            B.alloc(layout)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            B.dealloc(ptr, layout)
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            B.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static ALLOC: Upward = Upward;
}

#[cfg(test)]
mod tests {
    use super::Bump;
    use core::alloc::Layout;

    const WORD: usize = core::mem::size_of::<usize>();

    /// A `Bump` over a zeroed 8 KiB region (the region outlives the test body).
    fn with_bump(f: impl FnOnce(&Bump)) {
        let mut region = vec![0u64; 1024];
        let start = region.as_mut_ptr() as usize;
        let b = Bump {
            start,
            end: start + region.len() * 8,
        };
        f(&b);
        drop(region);
    }

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn first_block_skips_the_head_word_and_is_aligned() {
        for align in [1, 8, 16] {
            with_bump(|b| unsafe {
                let p = b.alloc(layout(3, align)) as usize;
                assert!(p >= b.start + WORD, "align {align}");
                assert_eq!(p % align, 0, "align {align}");
                assert!(
                    p < b.start + WORD + align,
                    "no more padding than the alignment"
                );
                let q = b.alloc(layout(5, align)) as usize;
                assert!(q >= p + 3 && q % align == 0);
            });
        }
    }

    #[test]
    fn the_last_block_grows_and_shrinks_in_place() {
        with_bump(|b| unsafe {
            let l = layout(16, 8);
            let p = b.alloc(l);
            p.write_bytes(7, 16);
            let grown = b.realloc(p, l, 64);
            assert_eq!(grown, p, "grown in place");
            let shrunk = b.realloc(grown, layout(64, 8), 8);
            assert_eq!(shrunk, p, "shrunk in place");
            // The space given back is handed out next.
            let q = b.alloc(layout(8, 8));
            assert_eq!(q as usize, p as usize + 8);
            assert_eq!(*p, 7);
        });
    }

    #[test]
    fn only_the_last_block_is_given_back() {
        with_bump(|b| unsafe {
            let l = layout(32, 8);
            let a = b.alloc(l);
            let c = b.alloc(l);
            // An older block stays allocated.
            b.dealloc(a, l);
            let d = b.alloc(l);
            assert_eq!(d as usize, c as usize + 32);
            // The last one is reused.
            b.dealloc(d, l);
            assert_eq!(b.alloc(l), d);
        });
    }

    #[test]
    fn an_older_block_is_copied_on_growth() {
        with_bump(|b| unsafe {
            let l = layout(16, 8);
            let a = b.alloc(l);
            for i in 0..16 {
                *a.add(i) = i as u8;
            }
            let c = b.alloc(l);
            let moved = b.realloc(a, l, 48);
            assert!(
                moved as usize >= c as usize + 16,
                "a fresh block after the last one"
            );
            for i in 0..16 {
                assert_eq!(*moved.add(i), i as u8, "contents preserved");
            }
            // Shrinking an older block keeps it where it is.
            assert_eq!(b.realloc(c, l, 8), c);
        });
    }

    #[test]
    fn exhaustion_returns_null() {
        with_bump(|b| unsafe {
            let room = b.end - (b.start + WORD);
            // One byte over the end fails and changes nothing.
            assert!(b.alloc(layout(room + 1, 1)).is_null());
            // Exactly up to the end succeeds.
            let p = b.alloc(layout(room, 1));
            assert_eq!(p as usize + room, b.end);
            assert!(b.alloc(layout(1, 1)).is_null());
        });
        with_bump(|b| unsafe {
            // The largest valid layout is refused, and sizes that overflow the
            // address space are null (`checked_add`), not a wrap.
            assert!(b.alloc(layout(isize::MAX as usize, 1)).is_null());
            let p = b.alloc(layout(8, 8));
            assert!(!p.is_null());
            assert!(b.realloc(p, layout(8, 8), usize::MAX).is_null());
            assert!(!b.alloc(layout(8, 8)).is_null());
        });
    }

    #[test]
    fn realloc_past_the_end_returns_null_and_keeps_the_block() {
        with_bump(|b| unsafe {
            let l = layout(16, 8);
            let p = b.alloc(l);
            p.write_bytes(9, 16);
            let room = b.end - p as usize;
            assert!(b.realloc(p, l, room + 1).is_null());
            assert_eq!(*p.add(15), 9, "the block is untouched");
            // The block is still the last one and can still grow up to the end.
            assert_eq!(b.realloc(p, l, room), p);
        });
        with_bump(|b| unsafe {
            // An older block that must move but does not fit: null, kept.
            let l = layout(16, 8);
            let a = b.alloc(l);
            a.write_bytes(5, 16);
            let _last = b.alloc(l);
            assert!(b.realloc(a, l, b.end - b.start).is_null());
            assert_eq!(*a.add(15), 5, "the block is untouched");
        });
    }
}
