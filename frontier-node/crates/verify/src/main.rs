//! `frontier-verify` (W1 skeleton: always "cannot verify", exit 2; wave 4 implements it).
fn main() {
    eprintln!("frontier-verify: skeleton (M1 wave 1); checks V1-V13 arrive in wave 4.");
    std::process::exit(verify_core::EXIT_UNVERIFIABLE);
}
