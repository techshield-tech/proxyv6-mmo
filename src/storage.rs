use std::fs;
use std::net::Ipv6Addr;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tracked {
    pub addr: Ipv6Addr,
    pub adapter_index: u32,
    pub adapter_name: String,
    pub prefix_len: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub persistent: bool,
    pub nodad: bool,
    pub ip_check_url: String,
    pub test_threads: usize,
    pub test_timeout_secs: u64,
    pub export_port: String,
    pub export_format: u8,
    pub count: usize,
    pub proxy_bind: String,
    pub proxy_start_port: u16,
    pub proxy_auth: bool,
    pub proxy_user: String,
    pub proxy_pass: String,
    pub sequential: bool,
    pub offset_text: String,
    pub export_scope: u8,
    pub only_global: bool,
    pub custom_prefix: bool,
    pub prefix_text: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: true,
            persistent: false,
            nodad: false,
            ip_check_url: "http://api6.ipify.org/".to_string(),
            test_threads: 16,
            test_timeout_secs: 5,
            export_port: "8080".to_string(),
            export_format: 0,
            count: 10,
            proxy_bind: "127.0.0.1".to_string(),
            proxy_start_port: 10000,
            proxy_auth: false,
            proxy_user: String::new(),
            proxy_pass: String::new(),
            sequential: false,
            offset_text: "0x1000".to_string(),
            export_scope: 2,
            only_global: false,
            custom_prefix: false,
            prefix_text: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Persisted {
    pub tracked: Vec<Tracked>,
    pub settings: Settings,
}

fn config_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("vn", "mmo-tool", "proxyv6-mmo")?;
    Some(dirs.config_dir().join("state.json"))
}

fn legacy_config_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("vn", "mmo-tool", "creator-ipv6")?;
    Some(dirs.config_dir().join("state.json"))
}

pub fn load() -> Persisted {
    let Some(mut path) = config_path() else {
        return Persisted::default();
    };
    if !path.exists() {
        if let Some(old) = legacy_config_path().filter(|p| p.exists()) {
            path = old;
        }
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(data: &Persisted) -> anyhow::Result<()> {
    let path = config_path().ok_or_else(|| anyhow::anyhow!("Không xác định được thư mục cấu hình"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(data)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn path_display() -> String {
    config_path().map(|p| p.display().to_string()).unwrap_or_default()
}
