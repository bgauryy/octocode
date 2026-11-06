//! Which network addresses Octocode may connect to on a caller's behalf.
//! One predicate for every outbound target a query names (a captured URL, a
//! custom registry): drift between copies would let one path reach an
//! internal network the other refuses.
//!
//! [`BLOCKS`] is the IANA IPv4 and IPv6 Special-Purpose Address Registries
//! (both last updated 2025-10-09) in one table, judged by the most specific
//! block that holds an address. IPv4 addresses are looked up in their
//! IPv4-mapped IPv6 form, so a mapped address is judged as the IPv4 address
//! a socket connects to.
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Publicly routable unicast: outside every special-purpose block that is
/// not globally reachable (loopback, private, shared, link-local,
/// documentation, benchmarking, translation, discard, reserved), multicast,
/// broadcast and anycast service blocks, inside the IPv6 global-unicast
/// allocation, and, for an IPv6 form that carries an IPv4 destination
/// (IPv4-mapped, NAT64, 6to4), with a public destination.
pub(crate) fn is_public(ip: IpAddr) -> bool {
    let bits = match ip {
        IpAddr::V4(ip) => ip.to_ipv6_mapped().to_bits(),
        IpAddr::V6(ip) => ip.to_bits(),
    };
    match reach(bits) {
        Reach::Global => true,
        Reach::Local => false,
        // The destination is plain IPv4: its block is Global or Local.
        Reach::Embeds { shift } => {
            // The shift leaves the embedded address in the low 32 bits.
            let low = u32::try_from((bits >> shift) & u128::from(u32::MAX)).unwrap_or(u32::MAX);
            let destination = Ipv4Addr::from_bits(low);
            reach(destination.to_ipv6_mapped().to_bits()) == Reach::Global
        }
    }
}

/// The one refusal for a host that is not public, does not resolve, or
/// resolves to any address that is not public. It names only `host` as the
/// caller gave it: naming a resolved address, or telling an unresolvable
/// name from a private one, would let a caller map an internal network.
pub(crate) fn not_public_refusal(host: &str) -> String {
    format!("{host} is refused: a target must be, or resolve only to, public addresses.")
}

/// Whether a datagram to an address in a block may leave the caller's
/// network, by the registry's Globally Reachable column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    Global,
    Local,
    /// Carries an IPv4 destination in bits `shift..shift + 32` (from the
    /// low end): a translator or relay delivers to it, so it decides.
    Embeds {
        shift: u32,
    },
}

#[derive(Debug)]
struct Block {
    /// The block's first address as IPv6 bits (IPv4 in mapped form).
    network: u128,
    /// Prefix length over the 128 bits.
    len: u32,
    reach: Reach,
}

const fn v4(a: u8, b: u8, c: u8, d: u8, len: u32, reach: Reach) -> Block {
    Block {
        network: Ipv4Addr::new(a, b, c, d).to_ipv6_mapped().to_bits(),
        len: 96 + len,
        reach,
    }
}

const fn v6(network: Ipv6Addr, len: u32, reach: Reach) -> Block {
    Block {
        network: network.to_bits(),
        len,
        reach,
    }
}

const fn addr(segments: [u16; 8]) -> Ipv6Addr {
    let [a, b, c, d, e, f, g, h] = segments;
    Ipv6Addr::new(a, b, c, d, e, f, g, h)
}

use Reach::{Embeds, Global, Local};

/// Every registry entry, with deliberate departures marked. Within a
/// Local block every more specific entry is Local too: the registry marks
/// anycast service entries (PCP, TURN, DNS-SD, AMT, AS112) globally
/// reachable, but an anycast address reaches the nearest instance, which
/// may sit inside the caller's network (PCP's is the caller's own gateway);
/// ORCHIDv2 and DET prefixes are identifiers, never locators (RFC 7343,
/// RFC 9374).
const BLOCKS: &[Block] = &[
    // IPv6 outside the global-unicast allocation 2000::/3 (IANA IPv6
    // Address Space): reserved and unallocated space, IPv4-compatible
    // (deprecated) and IPv4-translated (SIIT) forms, site-local
    // (deprecated) and multicast.
    v6(addr([0, 0, 0, 0, 0, 0, 0, 0]), 0, Local),
    v6(addr([0x2000, 0, 0, 0, 0, 0, 0, 0]), 3, Global),
    v6(addr([0, 0, 0, 0, 0, 0, 0, 1]), 128, Local),
    v6(addr([0, 0, 0, 0, 0, 0, 0, 0]), 128, Local),
    // IPv4-mapped: the registry marks it not reachable as an IPv6
    // destination, but a socket connects to the IPv4 address, so the IPv4
    // blocks below judge it; all other IPv4 unicast is public.
    v6(addr([0, 0, 0, 0, 0, 0xffff, 0, 0]), 96, Global),
    // NAT64 well-known prefix (RFC 6052): Global, toward its IPv4 address.
    v6(
        addr([0x64, 0xff9b, 0, 0, 0, 0, 0, 0]),
        96,
        Embeds { shift: 0 },
    ),
    // Local-use NAT64 (RFC 8215).
    v6(addr([0x64, 0xff9b, 1, 0, 0, 0, 0, 0]), 48, Local),
    // Discard-only (RFC 6666) and dummy prefix (RFC 9780).
    v6(addr([0x100, 0, 0, 0, 0, 0, 0, 0]), 64, Local),
    v6(addr([0x100, 0, 0, 1, 0, 0, 0, 0]), 64, Local),
    // IETF protocol assignments (RFC 2928).
    v6(addr([0x2001, 0, 0, 0, 0, 0, 0, 0]), 23, Local),
    // Teredo (registry: N/A): a tunnel to a host behind a NAT.
    v6(addr([0x2001, 0, 0, 0, 0, 0, 0, 0]), 32, Local),
    // PCP, TURN and DNS-SD SRP anycast (registry: Global; see above).
    v6(addr([0x2001, 1, 0, 0, 0, 0, 0, 1]), 128, Local),
    v6(addr([0x2001, 1, 0, 0, 0, 0, 0, 2]), 128, Local),
    v6(addr([0x2001, 1, 0, 0, 0, 0, 0, 3]), 128, Local),
    // Benchmarking (RFC 5180).
    v6(addr([0x2001, 2, 0, 0, 0, 0, 0, 0]), 48, Local),
    // AMT anycast and AS112-v6 (registry: Global; see above).
    v6(addr([0x2001, 3, 0, 0, 0, 0, 0, 0]), 32, Local),
    v6(addr([0x2001, 4, 0x112, 0, 0, 0, 0, 0]), 48, Local),
    // Deprecated ORCHID (terminated 2014).
    v6(addr([0x2001, 0x10, 0, 0, 0, 0, 0, 0]), 28, Local),
    // ORCHIDv2 and DETs (registry: Global; identifiers, see above).
    v6(addr([0x2001, 0x20, 0, 0, 0, 0, 0, 0]), 28, Local),
    v6(addr([0x2001, 0x30, 0, 0, 0, 0, 0, 0]), 28, Local),
    // Documentation (RFC 3849).
    v6(addr([0x2001, 0xdb8, 0, 0, 0, 0, 0, 0]), 32, Local),
    // 6to4 (registry: N/A): a relay delivers to the IPv4 address in bits
    // 16..48 (RFC 3056).
    v6(
        addr([0x2002, 0, 0, 0, 0, 0, 0, 0]),
        16,
        Embeds { shift: 80 },
    ),
    // Direct-delegation AS112 (registry: Global; anycast, see above).
    v6(addr([0x2620, 0x4f, 0x8000, 0, 0, 0, 0, 0]), 48, Local),
    // Documentation (RFC 9637).
    v6(addr([0x3fff, 0, 0, 0, 0, 0, 0, 0]), 20, Local),
    // SRv6 SIDs (RFC 9602).
    v6(addr([0x5f00, 0, 0, 0, 0, 0, 0, 0]), 16, Local),
    // Unique-local (RFC 4193) and link-local unicast.
    v6(addr([0xfc00, 0, 0, 0, 0, 0, 0, 0]), 7, Local),
    v6(addr([0xfe80, 0, 0, 0, 0, 0, 0, 0]), 10, Local),
    // IPv4.
    v4(0, 0, 0, 0, 8, Local),
    v4(0, 0, 0, 0, 32, Local),
    v4(10, 0, 0, 0, 8, Local),
    v4(100, 64, 0, 0, 10, Local),
    v4(127, 0, 0, 0, 8, Local),
    v4(169, 254, 0, 0, 16, Local),
    v4(172, 16, 0, 0, 12, Local),
    v4(192, 0, 0, 0, 24, Local),
    v4(192, 0, 0, 0, 29, Local),
    v4(192, 0, 0, 8, 32, Local),
    // PCP and TURN anycast (registry: Global; see above).
    v4(192, 0, 0, 9, 32, Local),
    v4(192, 0, 0, 10, 32, Local),
    v4(192, 0, 0, 170, 32, Local),
    v4(192, 0, 0, 171, 32, Local),
    v4(192, 0, 2, 0, 24, Local),
    // AS112-v4 and AMT anycast (registry: Global; see above).
    v4(192, 31, 196, 0, 24, Local),
    v4(192, 52, 193, 0, 24, Local),
    // Deprecated 6to4 relay anycast (terminated 2015, RFC 7526) and the
    // 6a44 relay within it.
    v4(192, 88, 99, 0, 24, Local),
    v4(192, 88, 99, 2, 32, Local),
    v4(192, 168, 0, 0, 16, Local),
    // Direct-delegation AS112 (registry: Global; anycast, see above).
    v4(192, 175, 48, 0, 24, Local),
    v4(198, 18, 0, 0, 15, Local),
    v4(198, 51, 100, 0, 24, Local),
    v4(203, 0, 113, 0, 24, Local),
    // Multicast (IANA IPv4 Multicast Address Space): never unicast.
    v4(224, 0, 0, 0, 4, Local),
    v4(240, 0, 0, 0, 4, Local),
    v4(255, 255, 255, 255, 32, Local),
];

fn mask(len: u32) -> u128 {
    u128::MAX.checked_shl(128 - len).unwrap_or(0)
}

/// The reach of the most specific block holding `bits`. `::/0` holds every
/// address, so the fallback is unreachable; it fails closed regardless.
fn reach(bits: u128) -> Reach {
    BLOCKS
        .iter()
        .filter(|block| bits & mask(block.len) == block.network)
        .max_by_key(|block| block.len)
        .map_or(Local, |block| block.reach)
}

#[cfg(test)]
mod tests;
