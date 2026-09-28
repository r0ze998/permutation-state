//! The §12 / §10.3 port rule: every port a stack binds is in 41000–41999,
//! never on the reserved list, and not already listened on (the `lsof`
//! rule, `fclient::ports::port_in_use`: IPv4, IPv6 and wildcard
//! listeners). The rule never requires the reserved ports to be idle
//! (I-26): the owner's own services listen on some of them.

use crate::config::Ports;

pub const RESERVED: [u16; 11] = [
    4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
];

/// The static half of the rule (reserved, range).
pub fn allowed(p: u16) -> Result<(), String> {
    if RESERVED.contains(&p) {
        return Err(format!("M1 port {p} is reserved: fix the config"));
    }
    if !(41_000..=41_999).contains(&p) {
        return Err(format!("M1 port {p} is outside 41000-41999"));
    }
    Ok(())
}

/// The whole rule for one port.
pub fn check(p: u16) -> Result<(), String> {
    allowed(p)?;
    if fclient::ports::port_in_use(p) {
        return Err(format!("M1 port {p} is busy"));
    }
    Ok(())
}

/// Every problem with a stack's ports (empty = it may start): the rule for
/// each, and no port named twice.
pub fn problems(ports: &Ports, busy_check: bool) -> Vec<String> {
    let mut out = vec![];
    let all = ports.all();
    for (i, (name, p)) in all.iter().enumerate() {
        let r = if busy_check { check(*p) } else { allowed(*p) };
        if let Err(e) = r {
            out.push(format!("{name}: {e}"));
        }
        if let Some((other, _)) = all[..i].iter().find(|(_, q)| q == p) {
            out.push(format!("{name}: port {p} is also {other}'s"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StackConfig;

    #[test]
    fn the_rule() {
        for p in RESERVED {
            assert!(allowed(p).is_err(), "{p}");
        }
        assert!(allowed(38_810).is_err());
        assert!(allowed(40_999).is_err());
        assert!(allowed(42_000).is_err());
        assert!(allowed(41_000).is_ok() && allowed(41_999).is_ok());
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let p = l.local_addr().unwrap().port();
        if (41_000..=41_999).contains(&p) {
            assert!(check(p).is_err(), "busy");
        }
    }

    #[test]
    fn configs_are_checked_whole() {
        let mut c = StackConfig::default();
        assert!(problems(&c.ports().unwrap(), false).is_empty());
        c.base_port = 41_990; // herald at 42030
        assert!(!problems(&c.ports().unwrap(), false).is_empty());
        let mut c = StackConfig::default();
        c.offsets.keeper_b = c.offsets.keeper_a;
        let p = problems(&c.ports().unwrap(), false);
        assert!(p.iter().any(|e| e.contains("also")), "{p:?}");
    }
}
