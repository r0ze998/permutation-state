//! Wylls kernels for the web client (M1 contract §9.5).
//!
//! Every export has the same C ABI over linear memory:
//!
//! ```text
//! name(in_ptr: *const u8, in_len: usize) -> *mut u8
//! ```
//!
//! The input is the borsh encoding of the export's argument struct (see
//! [`api`]); the answer is a **frame** the caller owns and releases with
//! `free(ptr, 5 + len)`:
//!
//! ```text
//! status u8 | len u32 LE | payload [len]
//! ```
//!
//! `status` is [`OK`] (payload = the borsh answer), [`BAD_INPUT`] (the input
//! does not decode; payload = a short ASCII reason), [`REFUSED`] (the kernel
//! refuses the input; payload = borsh `Refusal { code u8, arg u32 }`) or
//! [`UNAVAILABLE`] (the export exists but its kernel is not in this build).
//! `alloc(len)` hands the caller a buffer for the input; `free(ptr, len)`
//! releases inputs and frames alike.
//!
//! On `wasm32` the functions are exported unmangled; on the host they keep
//! Rust names (an unmangled `free` would replace the C library's), and the
//! host tests call them through the same frame protocol ([`call`]).

pub mod api;
mod path;

/// The export protocol's version (`abi_version()` answers it).
pub const ABI_VERSION: u32 = 1;

/// Frame status: the payload is the borsh answer.
pub const OK: u8 = 0;
/// Frame status: the input did not decode.
pub const BAD_INPUT: u8 = 1;
/// Frame status: the kernel refused the input (`api::Refusal`).
pub const REFUSED: u8 = 2;
/// Frame status: this export's kernel is not in this build.
pub const UNAVAILABLE: u8 = 3;

/// Length of a frame header: status u8 and len u32.
pub const FRAME_HEADER: usize = 5;

/// A finished answer before it is framed.
pub struct Answer {
    pub status: u8,
    pub payload: Vec<u8>,
}

impl Answer {
    pub fn ok(payload: Vec<u8>) -> Self {
        Answer {
            status: OK,
            payload,
        }
    }
}

fn frame(a: Answer) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER + a.payload.len());
    out.push(a.status);
    out.extend_from_slice(&(a.payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&a.payload);
    out
}

fn leak(v: Vec<u8>) -> *mut u8 {
    Box::into_raw(v.into_boxed_slice()) as *mut u8
}

/// Allocate `len` zeroed bytes for the caller (an export's input).
#[cfg_attr(target_arch = "wasm32", no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    leak(vec![0u8; len])
}

/// Release a buffer from [`alloc`] or a frame (`len` = its full length).
///
/// # Safety
/// `ptr` must come from [`alloc`] or an export with exactly this `len`, and
/// must not be used afterwards.
#[cfg_attr(target_arch = "wasm32", no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller hands back a boxed slice of exactly `len` bytes.
    drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Run `f` over the input at `ptr..ptr+len` and leak its framed answer.
///
/// # Safety
/// `ptr..ptr+len` must be readable (or `len == 0`).
unsafe fn run(ptr: *const u8, len: usize, f: fn(&[u8]) -> Answer) -> *mut u8 {
    let input: &[u8] = if len == 0 || ptr.is_null() {
        &[]
    } else {
        // SAFETY: guaranteed by the caller.
        unsafe { core::slice::from_raw_parts(ptr, len) }
    };
    leak(frame(f(input)))
}

/// The host's way in: frame `f(input)` exactly as an export would, then
/// read the frame back. Tests use it to hold the protocol, not only the
/// functions, to the contract.
pub fn call(f: unsafe extern "C" fn(*const u8, usize) -> *mut u8, input: &[u8]) -> Answer {
    let buf = alloc(input.len());
    // SAFETY: `buf` holds `input.len()` bytes from `alloc`.
    unsafe {
        core::ptr::copy_nonoverlapping(input.as_ptr(), buf, input.len());
    }
    // SAFETY: `buf..buf+len` is readable.
    let out = unsafe { f(buf, input.len()) };
    // SAFETY: `buf` came from `alloc` with this length.
    unsafe { free(buf, input.len()) };
    // SAFETY: an export's answer starts with a 5-byte header.
    let head = unsafe { core::slice::from_raw_parts(out, FRAME_HEADER) };
    let status = head[0];
    let n = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    // SAFETY: the frame is `FRAME_HEADER + n` bytes long.
    let payload = unsafe { core::slice::from_raw_parts(out.add(FRAME_HEADER), n) }.to_vec();
    // SAFETY: the frame came from an export with this length.
    unsafe { free(out, FRAME_HEADER + n) };
    Answer { status, payload }
}

macro_rules! exports {
    ($($name:ident),* $(,)?) => {
        $(
            #[doc = concat!("C-ABI export of [`api::", stringify!($name), "`] (frame protocol, see the crate docs).")]
            ///
            /// # Safety
            /// `ptr..ptr+len` must be readable (or `len == 0`).
            #[cfg_attr(target_arch = "wasm32", no_mangle)]
            pub unsafe extern "C" fn $name(ptr: *const u8, len: usize) -> *mut u8 {
                // SAFETY: forwarded from the caller.
                unsafe { run(ptr, len, api::$name) }
            }
        )*
        /// Every export's name, in the order of §9.5 (plus the helpers this
        /// crate adds), for the loader test and the size report.
        pub const EXPORTS: &[&str] = &[$(stringify!($name)),*];
    };
}

exports!(
    abi_version,
    ruleset_hash,
    province_of,
    province_centre,
    ring_of,
    wedge_of,
    region_of,
    generate_province,
    plan_path,
    path_cost,
    earliest_arrival_bell,
    check_arrival_bell,
    bell_at,
    bell_start,
    tlock_round,
    seed_round,
    plaintext_pack,
    plaintext_unpack,
    plaintext_validate,
    commit,
    salt_of,
    body_xor,
    seal_root,
    ct_hash,
    resolve_clash,
    resolve_from_inputs,
    reachable,
    accrual_at,
);
