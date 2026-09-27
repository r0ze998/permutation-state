//! `frontier-stack` (M1 contract §10.3, §12).
//!
//! **W1 skeleton.** Wave 5 builds `up/verify/tamper/load/report/down`. What
//! exists now is the port rule every gate uses, as `check-ports`:
//!
//! ```text
//! frontier-stack check-ports --ports 41010,41020,41030
//! ```
//! A port fails if it is reserved (4185, 4190, 4191, 4194, 18899, 17799,
//! 28899, 27799, 26699, 5185, 5191), outside 41000–41999, or already has a
//! listener on any address (`fclient::ports::port_in_use`, the §10.3 `lsof`
//! rule; a probe bind of 127.0.0.1 alone missed wildcard and IPv6
//! listeners). It never requires the reserved ports to be idle (I-26).
//! `--config <file>` (the stack TOML) arrives with W5.

pub const RESERVED: [u16; 11] = [
    4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
];

fn check(p: u16) -> Result<(), String> {
    if RESERVED.contains(&p) {
        return Err(format!("M1 port {p} is reserved: fix the config"));
    }
    if !(41_000..=41_999).contains(&p) {
        return Err(format!("M1 port {p} is outside 41000-41999"));
    }
    if fclient::ports::port_in_use(p) {
        return Err(format!("M1 port {p} is busy"));
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check-ports") => {
            let ports: Vec<u16> = match (args.get(1).map(String::as_str), args.get(2)) {
                (Some("--ports"), Some(list)) => list
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().parse().unwrap_or(0))
                    .collect(),
                _ => {
                    eprintln!("usage: frontier-stack check-ports --ports P1,P2,... (--config arrives in wave 5)");
                    std::process::exit(2)
                }
            };
            let mut bad = 0;
            for p in ports {
                if let Err(e) = check(p) {
                    eprintln!("{e}");
                    bad += 1;
                }
            }
            std::process::exit(if bad == 0 { 0 } else { 1 });
        }
        _ => {
            eprintln!("frontier-stack: skeleton (M1 wave 1): only `check-ports --ports ...`; up/verify/tamper/load/report/down arrive in wave 5.");
            std::process::exit(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_rule() {
        assert!(check(4185).is_err());
        assert!(check(38_810).is_err());
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let p = l.local_addr().unwrap().port();
        if (41_000..=41_999).contains(&p) {
            assert!(check(p).is_err(), "busy");
        }
    }
}
