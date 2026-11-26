use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const CONFIG: &str = "clientconfig.rpg.riotgames.com";
const CONFIG_PATH: &str = "/api/v1/config/public?app=Riot%20Client&patchline=KeystoneFoundationLiveWin&namespace=keystone.self_update";
pub const VOICE_HOST: &str = "euc-prod.pp.sgp.pvp.net";
const GEO: &str = "riot-geo.pas.si.riotgames.com";
pub const PERMISSIONS: &str = "permissions.sgp.pvp.net";
const DOMAINS: &[&str] = &[
    CONFIG,
    "auth.riotgames.com",
    "authenticate.riotgames.com",
    "entitlements.auth.riotgames.com",
    "account.riotgames.com",
    "api.account.riotgames.com",
    "data.riotgames.com",
    GEO,
    "sieve.services.riotcdn.net",
    "valorant.secure.dyn.riotcdn.net",
    "ks-foundation.secure.dyn.riotcdn.net",
    PERMISSIONS,
];
const FALLBACK_IPS: &[&str] = &[
    "104.16.206.131",
    "104.16.56.21",
    "104.18.157.37",
    "104.18.32.47",
    "104.16.132.229",
    "104.17.25.14",
    "104.19.222.79",
    "104.21.48.1",
    "162.159.135.42",
    "162.159.136.42",
    "172.64.155.1",
    "172.67.70.1",
    "188.114.96.3",
    "188.114.97.3",
];
const TRIES: usize = 3;
const CONCURRENCY: usize = 6;
const FALLBACK_CONCURRENCY: usize = 3;

#[derive(Debug, Default)]
pub struct Report {
    pub pins: BTreeMap<String, Ipv4Addr>,
    pub unresolved: usize,
}

enum Resolution {
    StableDns,
    Pin(Ipv4Addr),
    Unresolved,
}

fn resolve(domain: &str, external: bool) -> BTreeSet<Ipv4Addr> {
    let public = [
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(8, 8, 8, 8),
        Ipv4Addr::new(77, 88, 8, 8),
    ];
    let servers = if external {
        public.map(Some).to_vec()
    } else {
        vec![None]
    };
    servers
        .into_iter()
        .flat_map(|server| crate::windows::dns_a(domain, server))
        .filter(|ip| {
            let [a, b, ..] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || a == 0
                || a >= 224
                || (a == 100 && (64..128).contains(&b)))
        })
        .collect()
}

fn accepts_status(domain: &str, status: u16) -> bool {
    if domain == CONFIG {
        status == 200
    } else {
        (200..400).contains(&status)
            || matches!(status, 401 | 404 | 405)
            // This endpoint can reject anonymous requests after a successful TLS handshake.
            || (domain == PERMISSIONS && status == 403)
    }
}

enum ProbeError {
    Http(u16),
    Transport(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(code) => write!(f, "HTTP {code}"),
            Self::Transport(message) => f.write_str(message),
        }
    }
}

fn probe(domain: &str, ip: Option<Ipv4Addr>) -> Result<Duration, ProbeError> {
    probe_https(domain, ip, false)
}

fn probe_https(
    domain: &str,
    ip: Option<Ipv4Addr>,
    transport_only: bool,
) -> Result<Duration, ProbeError> {
    let target = match domain {
        CONFIG => CONFIG_PATH,
        GEO => "/pas/v1/product/valorant",
        _ => "/",
    };
    let start = Instant::now();
    let code = (|| -> Result<u16, curl::Error> {
        let mut request = crate::http::request(&format!("https://{domain}{target}"), 7, 4)?;
        if let Some(ip) = ip {
            let mut addresses = curl::easy::List::new();
            addresses.append(&format!("{domain}:443:{ip}"))?;
            request.resolve(addresses)?;
        }
        request.write_function(|data| Ok(data.len()))?;
        request.perform()?;
        Ok(request.response_code()? as u16)
    })()
    .map_err(|error| ProbeError::Transport(error.to_string()))?;
    if !(if transport_only {
        (100..600).contains(&code)
    } else {
        accepts_status(domain, code)
    }) {
        return Err(ProbeError::Http(code));
    }
    Ok(start.elapsed())
}

fn check_ip(domain: &str, ip: Ipv4Addr) -> (Option<Duration>, bool, String) {
    let mut elapsed = Duration::ZERO;
    for attempt in 0..TRIES {
        match probe(domain, Some(ip)) {
            Ok(duration) => elapsed += duration,
            Err(ProbeError::Http(403)) => {
                return (
                    None,
                    true,
                    localized!(
                        format,
                        "HTTP 403: HTTPS доступен, сервер отказал в доступе (попытка {})",
                        "HTTP 403: HTTPS reachable, access denied (attempt {})",
                        attempt + 1
                    ),
                );
            }
            Err(error) => {
                return (
                    None,
                    false,
                    localized!(
                        format,
                        "Попытка {}: {error}",
                        "Attempt {}: {error}",
                        attempt + 1
                    ),
                );
            }
        }
    }
    let average = elapsed / TRIES as u32;
    (
        Some(average),
        false,
        localized!(
            format,
            "{TRIES}/{TRIES} ({} мс)",
            "{TRIES}/{TRIES} ({} ms)",
            average.as_millis()
        ),
    )
}

fn is_cloudflare(ip: Ipv4Addr) -> bool {
    matches!(
        ip.octets(),
        [104, 16..=31, ..]
            | [172, 64..=71, ..]
            | [162, 158..=159, ..]
            | [188, 114, 96..=99, _]
            | [198, 41, 128..=255, _]
            | [141, 101, 64..=127, _]
    )
}

fn check_domain(domain: &str) -> (Resolution, String) {
    use std::fmt::Write;
    let dns = resolve(domain, false);
    let mut tested = BTreeSet::new();
    let mut best = None;
    let mut details = format!("\n{domain}\n");
    let mut stable = true;
    let mut denied = false;

    for &ip in &dns {
        tested.insert(ip);
        let (average, forbidden, note) = check_ip(domain, ip);
        denied |= forbidden;
        let _ = writeln!(details, "  {ip:<15} {note}");
        if let Some(average) = average {
            if best.is_none_or(|(time, _)| average < time) {
                best = Some((average, ip));
            }
        } else {
            stable = false;
        }
    }
    if stable && !dns.is_empty() {
        details.push_str(crate::locale::text(
            "  DNS-адреса прошли проверку.\n",
            "  DNS addresses passed the check.\n",
        ));
        return (Resolution::StableDns, details);
    }

    if best.is_none() {
        let public = resolve(domain, true);
        let cf_domain = DOMAINS[..7].contains(&domain)
            || domain == "sieve.services.riotcdn.net"
            || dns.iter().chain(&public).copied().any(is_cloudflare);
        let fallback = FALLBACK_IPS.iter().filter_map(|value| value.parse().ok());
        let candidates: Vec<_> = public.into_iter().filter(|ip| tested.insert(*ip)).collect();
        let fallback: Vec<_> = fallback.filter(|ip| tested.insert(*ip)).collect();
        for (index, addresses) in [candidates, fallback].into_iter().enumerate() {
            if index == 1 && (domain == PERMISSIONS || !cf_domain || denied || best.is_some()) {
                break;
            }
            for batch in addresses.chunks(FALLBACK_CONCURRENCY) {
                best = thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter()
                        .map(|&ip| (ip, scope.spawn(move || check_ip(domain, ip))))
                        .collect();
                    let mut first_working = None;
                    for (ip, handle) in handles {
                        let (average, forbidden, note) = handle.join().expect("IP check");
                        denied |= forbidden;
                        let _ = writeln!(details, "  {ip:<15} {note}");
                        if let Some(average) = average {
                            first_working.get_or_insert((average, ip));
                        }
                    }
                    first_working
                });
                if best.is_some() {
                    break;
                }
            }
        }
    }

    if let Some((_, ip)) = best {
        let _ = writeln!(
            details,
            "{}",
            localized!(
                format,
                "  Подходящий адрес для hosts: {ip}",
                "  Suitable hosts address: {ip}"
            )
        );
        (Resolution::Pin(ip), details)
    } else if denied {
        details.push_str(crate::locale::text(
            "  HTTPS доступен, но сервер отклонил запрос. Файл hosts оставлен без изменений.\n",
            "  HTTPS reachable; the server denied this request. No hosts override.\n",
        ));
        (Resolution::Unresolved, details)
    } else {
        details.push_str(crate::locale::text(
            "  Стабильный адрес не найден.\n",
            "  No stable address found.\n",
        ));
        (Resolution::Unresolved, details)
    }
}

fn domains_from_logs() -> BTreeSet<String> {
    let mut domains = BTreeSet::new();
    let Some(local) = env::var_os("LOCALAPPDATA") else {
        return domains;
    };
    let directory = PathBuf::from(local).join(r"Riot Games\Riot Client\Logs\Riot Client Logs");
    let Ok(files) = fs::read_dir(directory) else {
        return domains;
    };
    let mut logs: Vec<_> = files
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != "log" {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, path))
        })
        .collect();
    logs.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let suffixes = [
        ".riotgames.com",
        ".riotcdn.net",
        ".pvp.net",
        ".playvalorant.com",
        ".leagueoflegends.com",
        ".rgpub.io",
    ];
    for (_, path) in logs.into_iter().take(5) {
        let Ok(bytes) = fs::read(path) else { continue };
        for line in String::from_utf8_lossy(&bytes).lines() {
            let line = line.to_ascii_lowercase();
            if ![
                "ssl failure",
                "zero certificates",
                "tls",
                "timed out",
                "connection reset",
                "failed to connect",
            ]
            .iter()
            .any(|error| line.contains(error))
            {
                continue;
            }
            for rest in line.split("://").skip(1) {
                let host = rest
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '.')
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('.');
                if suffixes.iter().any(|suffix| host.ends_with(suffix)) && host.len() < 254 {
                    domains.insert(host.to_owned());
                }
            }
        }
    }
    domains
}

pub enum VoiceRoute {
    Healthy,
    Candidate(Ipv4Addr),
    Uncertain,
}

pub fn check_voice() -> VoiceRoute {
    let addresses = resolve(VOICE_HOST, false);
    if addresses.len() < 2 {
        localized!(
            println,
            "Для {VOICE_HOST} нужны хотя бы два DNS-адреса.",
            "Need at least two DNS addresses for {VOICE_HOST}."
        );
        return VoiceRoute::Uncertain;
    }

    let results = thread::scope(|scope| {
        addresses
            .into_iter()
            .map(|ip| {
                scope.spawn(move || {
                    let passed = (0..TRIES)
                        .filter(|_| probe_https(VOICE_HOST, Some(ip), true).is_ok())
                        .count();
                    (ip, passed)
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|worker| worker.join().expect("voice HTTPS probe"))
            .collect::<Vec<_>>()
    });

    for (ip, passed) in &results {
        println!("{ip}: HTTPS {passed}/{TRIES}");
    }
    if results.iter().all(|&(_, passed)| passed == TRIES) {
        return VoiceRoute::Healthy;
    }
    if results.iter().any(|&(_, passed)| passed == 0)
        && let Some(&(ip, _)) = results.iter().find(|&&(_, passed)| passed == TRIES)
    {
        return VoiceRoute::Candidate(ip);
    }
    VoiceRoute::Uncertain
}

pub fn check_config() -> bool {
    match probe(CONFIG, None) {
        Ok(duration) => {
            localized!(
                println,
                "{CONFIG} HTTP 200 ({} мс, напрямую)",
                "{CONFIG} HTTP 200 ({} ms, direct)",
                duration.as_millis()
            );
            true
        }
        Err(error) => {
            eprintln!("{CONFIG}: {error}");
            false
        }
    }
}

pub fn run() -> Report {
    let mut domains = domains_from_logs();
    domains.extend(DOMAINS.iter().map(|name| (*name).to_owned()));
    let domains: Vec<_> = domains.into_iter().collect();
    let mut report = Report::default();
    localized!(
        println,
        "Проверка {} доменов Riot по HTTPS напрямую",
        "Checking {} Riot domains over direct HTTPS",
        domains.len()
    );
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (sender, receiver) = std::sync::mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..CONCURRENCY.min(domains.len()) {
            let sender = sender.clone();
            let domains = &domains;
            let next = &next;
            scope.spawn(move || {
                while let Some(domain) =
                    domains.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                {
                    let (resolution, details) = check_domain(domain);
                    if sender.send((domain, resolution, details)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (domain, resolution, details) in receiver {
            print!("{details}");
            match resolution {
                Resolution::StableDns => {}
                Resolution::Pin(ip) => {
                    report.pins.insert(domain.clone(), ip);
                }
                Resolution::Unresolved => report.unresolved += 1,
            }
        }
    });
    report
}
