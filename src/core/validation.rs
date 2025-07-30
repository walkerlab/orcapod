use crate::uniffi::error::{Result, selector};
use snafu::OptionExt as _;
use std::collections::{HashMap, HashSet};

pub fn validate_packet<SV, PV>(
    kind: String,
    spec: &HashMap<String, SV>,
    packet: &HashMap<String, PV>,
) -> Result<()> {
    let spec_keys = spec.keys().collect::<HashSet<_>>();
    let packet_keys = packet.keys().collect::<HashSet<_>>();
    let missing_keys = spec_keys
        .difference(&packet_keys)
        .copied()
        .cloned()
        .collect::<Vec<_>>();

    missing_keys
        .is_empty()
        .then_some(())
        .context(selector::IncompletePacket { kind, missing_keys })?;

    Ok(())
}
