use super::*;

fn assert_public(addresses: &[&str]) {
    for address in addresses {
        assert!(is_public(address.parse().unwrap()), "{address} is public");
    }
}

fn assert_refused(addresses: &[&str]) {
    for address in addresses {
        assert!(!is_public(address.parse().unwrap()), "{address} is refused");
    }
}

/// Each block is a well-formed prefix (no bits past its length) listed
/// once, so the most specific match is unambiguous.
#[test]
fn every_block_is_a_distinct_prefix() {
    for (index, block) in BLOCKS.iter().enumerate() {
        assert!(block.len <= 128, "{block:?}");
        assert_eq!(block.network & !mask(block.len), 0, "{block:?}");
        assert!(
            BLOCKS[..index]
                .iter()
                .all(|earlier| (earlier.network, earlier.len) != (block.network, block.len)),
            "{block:?} is listed twice"
        );
    }
}

#[test]
fn ordinary_unicast_and_embeddings_of_it_pass() {
    assert_public(&[
        "93.184.216.34",
        "8.8.8.8",
        "100.128.0.1",
        "100.63.255.255",
        "198.17.255.255",
        "198.20.0.0",
        "192.88.98.255",
        "192.88.100.0",
        "223.255.255.255",
        "2606:4700::1111",
        "2a00:1450::1",
        "::ffff:93.184.216.34",
        "64:ff9b::808:808",
        "2002:5db8:d822::1",
        // Next to the IETF block 2001::/23, outside it.
        "2001:200::1",
        // Next to 64:ff9b:1::/48 and 3fff::/20, outside them.
        "3fff:1000::1",
    ]);
}

/// Every IPv4 Special-Purpose Address Registry entry whose Globally
/// Reachable value is False, the terminated 6to4 relay anycast block,
/// multicast, and the anycast service blocks whose nearest instance may
/// sit inside the caller's network.
#[test]
fn ipv4_special_purpose_blocks_are_refused() {
    assert_refused(&[
        "0.0.0.0",
        "0.1.2.3",
        "10.1.2.3",
        "100.64.0.1",
        "100.127.255.255",
        "127.0.0.1",
        "127.255.255.254",
        "169.254.169.254",
        "172.16.0.1",
        "172.31.255.255",
        "192.0.0.1",
        "192.0.0.8",
        "192.0.0.9",
        "192.0.0.10",
        "192.0.0.170",
        "192.0.0.171",
        "192.0.0.255",
        "192.0.2.1",
        "192.31.196.1",
        "192.52.193.1",
        "192.88.99.1",
        "192.88.99.2",
        "192.168.1.1",
        "192.175.48.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.254",
        "255.255.255.255",
    ]);
}

/// Every IPv6 Special-Purpose Address Registry entry whose Globally
/// Reachable value is False, the blocks inside 2001::/23 it carves out
/// (anycast services, identifiers that are no locators, Teredo), and all
/// space outside the global-unicast allocation 2000::/3.
#[test]
fn ipv6_special_purpose_blocks_are_refused() {
    assert_refused(&[
        "::",
        "::1",
        "64:ff9b:1::1",
        "64:ff9b:1:ffff::808:808",
        "100::1",
        "100:0:0:1::1",
        "2001::1",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        "2001:1::1",
        "2001:1::2",
        "2001:1::3",
        "2001:1::4",
        "2001:2::1",
        "2001:3::1",
        "2001:4:112::1",
        "2001:5::1",
        "2001:10::1",
        "2001:20::1",
        "2001:30::1",
        "2001:1ff:ffff::1",
        "2001:db8::1",
        "2620:4f:8000::1",
        "3fff::1",
        "3fff:fff::1",
        "5f00::1",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        "febf::1",
        // Outside the global-unicast allocation 2000::/3.
        "fec0::1",
        "ff02::1",
        "ff0e::1",
        "4000::1",
        "1::1",
        "::127.0.0.1",
        "::8.8.8.8",
        // IPv4-translated (SIIT, RFC 6145) ::ffff:0:a.b.c.d.
        "::ffff:0:a00:1",
        "::ffff:0:808:808",
    ]);
}

/// An IPv6 address that carries an IPv4 destination (IPv4-mapped, NAT64
/// well-known prefix, 6to4) is judged by that destination.
#[test]
fn embedded_ipv4_destinations_are_judged_as_ipv4() {
    assert_refused(&[
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:100.64.0.1",
        "::ffff:169.254.169.254",
        "::ffff:192.88.99.1",
        "::ffff:198.18.0.1",
        "::ffff:240.0.0.1",
        "64:ff9b::a00:1",
        "64:ff9b::7f00:1",
        "64:ff9b::a9fe:a9fe",
        "64:ff9b::6440:1",
        "64:ff9b::c058:6301",
        "2002:c0a8:0101::1",
        "2002:7f00:1::1",
        "2002:a9fe:a9fe::1",
        "2002:c612::1",
    ]);
    assert_public(&["::ffff:8.8.8.8", "64:ff9b::5db8:d822", "2002:808:808::1"]);
}
