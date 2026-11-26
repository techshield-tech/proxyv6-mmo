use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};

use netdev::interface::state::OperState;
use netdev::interface::types::InterfaceType;

use super::{is_global_unicast, network_of};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    Wired,
    Wifi,
    Virtual,
    Other,
}

impl AdapterKind {
    pub fn label(self) -> &'static str {
        match self {
            AdapterKind::Wired => "Có dây",
            AdapterKind::Wifi => "WiFi",
            AdapterKind::Virtual => "Ảo",
            AdapterKind::Other => "Khác",
        }
    }
}

#[derive(Clone, Debug)]
pub struct V6Entry {
    pub addr: Ipv6Addr,
    pub prefix_len: u8,
    pub temporary: bool,
    pub tentative: bool,
}

impl V6Entry {
    pub fn is_global(&self) -> bool {
        is_global_unicast(&self.addr)
    }
}

#[derive(Clone, Debug)]
pub struct Adapter {
    pub index: u32,
    pub name: String,
    pub friendly: String,
    pub description: String,
    pub mac: Option<String>,
    pub up: bool,
    pub kind: AdapterKind,
    pub ipv4: Vec<(Ipv4Addr, u8)>,
    pub ipv6: Vec<V6Entry>,
    pub v6_gateway: bool,
}

#[derive(Clone, Debug)]
pub struct DetectedPrefix {
    pub network: Ipv6Addr,
    pub prefix_len: u8,
}

impl Adapter {
    pub fn display_name(&self) -> &str {
        if self.friendly.is_empty() {
            &self.name
        } else {
            &self.friendly
        }
    }

    pub fn has_global(&self) -> bool {
        self.ipv6.iter().any(V6Entry::is_global)
    }

    pub fn global_addrs(&self) -> impl Iterator<Item = &V6Entry> {
        self.ipv6.iter().filter(|e| e.is_global())
    }

    pub fn only_single_host_global(&self) -> bool {
        self.has_global() && self.global_addrs().all(|e| e.prefix_len >= 128)
    }

    pub fn detected_prefix(&self) -> Option<DetectedPrefix> {
        let entry = self
            .global_addrs()
            .find(|e| !e.temporary)
            .or_else(|| self.global_addrs().next())?;
        let len = if entry.prefix_len == 0 || entry.prefix_len >= 128 {
            64
        } else {
            entry.prefix_len
        };
        Some(DetectedPrefix {
            network: network_of(entry.addr, len),
            prefix_len: len,
        })
    }
}

fn looks_wifi(text: &str) -> bool {
    ["wi-fi", "wifi", "wireless", "wlan", "802.11"].iter().any(|k| text.contains(k))
}

fn looks_virtual(text: &str) -> bool {
    [
        "virtual", "vmware", "vbox", "virtualbox", "hyper-v", "veth", "docker", "br-", "virbr", "vethernet",
        "tap", "tun", "zerotier", "tailscale", "wireguard",
    ]
    .iter()
    .any(|k| text.contains(k))
}

fn classify(iface: &netdev::Interface) -> AdapterKind {
    let text = format!(
        "{} {} {}",
        iface.name.to_lowercase(),
        iface.friendly_name.clone().unwrap_or_default().to_lowercase(),
        iface.description.clone().unwrap_or_default().to_lowercase()
    );
    match iface.if_type {
        InterfaceType::Wireless80211 => AdapterKind::Wifi,
        InterfaceType::Bridge | InterfaceType::ProprietaryVirtual | InterfaceType::Tunnel => AdapterKind::Virtual,
        InterfaceType::Ethernet
        | InterfaceType::GigabitEthernet
        | InterfaceType::FastEthernetT
        | InterfaceType::FastEthernetFx
        | InterfaceType::Ethernet3Megabit => {
            if looks_wifi(&text) {
                AdapterKind::Wifi
            } else if looks_virtual(&text) {
                AdapterKind::Virtual
            } else {
                AdapterKind::Wired
            }
        }
        _ => {
            let n = iface.name.to_lowercase();
            if n.starts_with("wl") || looks_wifi(&text) {
                AdapterKind::Wifi
            } else if n.starts_with("en") || n.starts_with("eth") {
                AdapterKind::Wired
            } else if looks_virtual(&text) {
                AdapterKind::Virtual
            } else {
                AdapterKind::Other
            }
        }
    }
}

fn convert(iface: &netdev::Interface) -> Adapter {
    let ipv6 = iface
        .ipv6
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let flags = iface.ipv6_addr_flags.get(i).copied().unwrap_or_default();
            V6Entry {
                addr: net.addr(),
                prefix_len: net.prefix_len(),
                temporary: flags.temporary,
                tentative: flags.tentative,
            }
        })
        .collect();
    Adapter {
        index: iface.index,
        name: iface.name.clone(),
        friendly: iface.friendly_name.clone().unwrap_or_default(),
        description: iface.description.clone().unwrap_or_default(),
        mac: iface.mac_addr.map(|m| m.to_string()),
        up: iface.oper_state == OperState::Up || (iface.oper_state == OperState::Unknown && iface.is_up()),
        kind: classify(iface),
        ipv4: iface.ipv4.iter().map(|n| (n.addr(), n.prefix_len())).collect(),
        ipv6,
        v6_gateway: iface.gateway.as_ref().is_some_and(|g| !g.ipv6.is_empty()),
    }
}

pub fn list_adapters() -> Vec<Adapter> {
    let mut list: Vec<Adapter> = netdev::get_interfaces()
        .iter()
        .filter(|i| !i.is_loopback() && i.if_type != InterfaceType::Loopback)
        .map(convert)
        .collect();
    list.sort_by(|a, b| {
        b.has_global()
            .cmp(&a.has_global())
            .then(b.up.cmp(&a.up))
            .then(a.display_name().cmp(b.display_name()))
    });
    list
}

pub fn all_existing_v6() -> HashSet<Ipv6Addr> {
    netdev::get_interfaces()
        .iter()
        .flat_map(|i| i.ipv6.iter().map(|n| n.addr()))
        .collect()
}

pub fn addrs_on_index(index: u32) -> HashSet<Ipv6Addr> {
    netdev::get_interfaces()
        .iter()
        .filter(|i| i.index == index)
        .flat_map(|i| i.ipv6.iter().map(|n| n.addr()))
        .collect()
}
