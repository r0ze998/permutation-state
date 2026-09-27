//! Clashes (§5.11): GatherClash, ResolveFromInputs, ResolveClash (feature
//! `oracle`, tests only), SkipQuiet, CloseClashInputs, CloseArrivalDay,
//! CloseArrivalSlot. Stubs until W4-A.

super::stubs!(
    gather_clash,
    resolve_from_inputs,
    skip_quiet,
    close_clash_inputs,
    close_arrival_day,
    close_arrival_slot,
);

#[cfg(feature = "oracle")]
super::stubs!(resolve_clash);
