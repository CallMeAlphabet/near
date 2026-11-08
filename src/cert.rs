use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::{IpAddr, Ipv4Addr},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use base64::Engine;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, PKCS_ECDSA_P256_SHA256,
};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

pub struct Certs {
    pub ca_path: PathBuf,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub fingerprint: String,
    pub addresses: Vec<String>,
}

fn lan_addresses() -> Vec<String> {
    let mut found: Vec<IpAddr> = Vec::new();
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return vec!["127.0.0.1".to_string()];
        }
        let mut cursor = head;
        while !cursor.is_null() {
            let entry = &*cursor;
            if !entry.ifa_addr.is_null() && (*entry.ifa_addr).sa_family as i32 == libc::AF_INET {
                let socket = &*(entry.ifa_addr as *const libc::sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(socket.sin_addr.s_addr));
                if !ip.is_loopback() && !ip.is_unspecified() {
                    found.push(IpAddr::V4(ip));
                }
            }
            cursor = entry.ifa_next;
        }
        libc::freeifaddrs(head);
    }
    found.sort();
    found.dedup();
    if found.is_empty() {
        return vec!["127.0.0.1".to_string()];
    }
    found.iter().map(|ip| ip.to_string()).collect()
}

fn write_private(path: &Path, contents: &str) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

fn ca_der(pem: &str) -> Result<Vec<u8>> {
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    Ok(base64::engine::general_purpose::STANDARD.decode(body)?)
}

fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn generate(dir: &Path, addresses: &[String]) -> Result<()> {
    let now = OffsetDateTime::now_utc();

    let mut ca_params = CertificateParams::default();
    let mut ca_name = DistinguishedName::new();
    ca_name.push(DnType::CommonName, "near local ca");
    ca_params.distinguished_name = ca_name;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    ca_params.not_before = now - Duration::days(1);
    ca_params.not_after = now + Duration::days(3650);
    let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let ca_cert: Certificate = ca_params.self_signed(&ca_key)?;

    let mut sans = vec![
        "near.local".to_string(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ];
    sans.extend(addresses.iter().cloned());

    let mut leaf_params = CertificateParams::new(sans)?;
    let mut leaf_name = DistinguishedName::new();
    leaf_name.push(DnType::CommonName, "near");
    leaf_params.distinguished_name = leaf_name;
    leaf_params.is_ca = IsCa::ExplicitNoCa;
    leaf_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.not_before = now - Duration::days(1);
    leaf_params.not_after = now + Duration::days(825);
    let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let leaf_cert = leaf_params.signed_by(&leaf_key, &ca_cert, &ca_key)?;

    write_private(&dir.join("ca.key"), &ca_key.serialize_pem())?;
    fs::write(dir.join("ca.crt"), ca_cert.pem())?;
    write_private(&dir.join("server.key"), &leaf_key.serialize_pem())?;
    fs::write(dir.join("server.crt"), leaf_cert.pem())?;
    Ok(())
}

pub fn ensure(dir: &Path) -> Result<Certs> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;

    let ca_path = dir.join("ca.crt");
    let cert_path = dir.join("server.crt");
    let key_path = dir.join("server.key");
    let addresses = lan_addresses();

    if !ca_path.is_file() || !cert_path.is_file() || !key_path.is_file() {
        generate(dir, &addresses).context("failed to generate the local certificate authority")?;
    }

    let ca_pem = fs::read_to_string(&ca_path)?;
    let der = ca_der(&ca_pem).context("the local CA certificate is not valid PEM")?;
    let fingerprint = fingerprint(&der);

    Ok(Certs { ca_path, cert_path, key_path, fingerprint, addresses })
}
