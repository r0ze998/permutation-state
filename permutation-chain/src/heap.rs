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

pub const HEAP_BYTES: usize = 256 * 1024;

#[cfg(all(feature = "custom-heap", target_os = "solana"))]
mod upward {
    use super::HEAP_BYTES;
    use core::alloc::{GlobalAlloc, Layout};
    use solana_program::entrypoint::HEAP_START_ADDRESS;

    /// The first word of the heap stores the next free address; 0 = unused.
    pub struct Upward;

    const START: usize = HEAP_START_ADDRESS as usize;
    const END: usize = START + HEAP_BYTES;
    const HEAD: usize = core::mem::size_of::<usize>();

    #[inline]
    unsafe fn next() -> *mut usize {
        START as *mut usize
    }

    #[inline]
    fn align_up(p: usize, align: usize) -> usize {
        (p + align - 1) & !(align - 1)
    }

    unsafe impl GlobalAlloc for Upward {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let mut pos = *next();
            if pos == 0 {
                pos = START + HEAD;
            }
            let at = align_up(pos, layout.align());
            let end = match at.checked_add(layout.size()) {
                Some(e) if e <= END => e,
                _ => return core::ptr::null_mut(),
            };
            *next() = end;
            at as *mut u8
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // Give back the most recent allocation; bump otherwise.
            if ptr as usize + layout.size() == *next() {
                *next() = ptr as usize;
            }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let p = ptr as usize;
            if p + layout.size() == *next() {
                // The last block: grow or shrink in place.
                return match p.checked_add(new_size) {
                    Some(e) if e <= END => {
                        *next() = e;
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

    #[global_allocator]
    static ALLOC: Upward = Upward;
}
