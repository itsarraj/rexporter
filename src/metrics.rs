//! Pure parsers: each takes the *contents* of a `/proc` file as `&str` and
//! returns structured data, with zero file I/O in this module — the actual
//! `fs::read_to_string` calls live in `collect.rs`. That split is what
//! makes every parser testable with a plain string fixture, with no
//! dependency on the host actually being Linux with a populated `/proc`.
//!
//! Metric names deliberately match `node_exporter`'s own naming
//! (`node_cpu_seconds_total`, `node_memory_MemTotal_bytes`, ...) rather
//! than inventing a new schema — the point is that existing Grafana
//! dashboards built for `node_exporter` mostly work unmodified against
//! this exporter's `/metrics` output.

use std::collections::BTreeMap;

/// Jiffies-per-second. Not read from `sysconf(_SC_CLK_TCK)` (would need a
/// libc call) — hardcoded at 100, which is the value on effectively every
/// Linux system in practice. Documented as a known limitation rather than
/// silently assumed away: an exotic kernel build with a different `HZ`
/// would report CPU seconds scaled incorrectly.
const USER_HZ: f64 = 100.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuTimes {
    pub user: f64,
    pub nice: f64,
    pub system: f64,
    pub idle: f64,
    pub iowait: f64,
    pub irq: f64,
    pub softirq: f64,
    pub steal: f64,
}

/// Parses `/proc/stat`'s per-core `cpuN` lines (the aggregate `cpu` line,
/// with no number, is skipped — it's the sum of the per-core lines, and
/// Prometheus counters are meant to be summed downstream, not double
/// counted here).
pub fn parse_stat(contents: &str) -> BTreeMap<String, CpuTimes> {
    let mut result = BTreeMap::new();
    for line in contents.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            continue;
        };
        if rest.starts_with(' ') || rest.is_empty() {
            continue; // the aggregate "cpu " line, not a numbered core
        }
        let mut parts = rest.split_whitespace();
        let Some(core_str) = parts.next() else {
            continue;
        };
        if !core_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let fields: Vec<u64> = parts.filter_map(|f| f.parse().ok()).collect();
        if fields.len() < 8 {
            continue;
        }
        let jiffies_to_secs = |j: u64| j as f64 / USER_HZ;
        result.insert(
            core_str.to_string(),
            CpuTimes {
                user: jiffies_to_secs(fields[0]),
                nice: jiffies_to_secs(fields[1]),
                system: jiffies_to_secs(fields[2]),
                idle: jiffies_to_secs(fields[3]),
                iowait: jiffies_to_secs(fields[4]),
                irq: jiffies_to_secs(fields[5]),
                softirq: jiffies_to_secs(fields[6]),
                steal: jiffies_to_secs(fields[7]),
            },
        );
    }
    result
}

/// `/proc/meminfo` values are in kB; returned map is in bytes, keyed by the
/// field name verbatim (`MemTotal`, `MemFree`, ...) so callers pick which
/// subset to expose without this parser hardcoding an allowlist.
pub fn parse_meminfo(contents: &str) -> BTreeMap<String, u64> {
    let mut result = BTreeMap::new();
    for line in contents.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let kb: u64 = match rest.split_whitespace().next().and_then(|v| v.parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        result.insert(key.to_string(), kb.saturating_mul(1024));
    }
    result
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LoadAvg {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
}

pub fn parse_loadavg(contents: &str) -> Option<LoadAvg> {
    let mut parts = contents.split_whitespace();
    Some(LoadAvg {
        load1: parts.next()?.parse().ok()?,
        load5: parts.next()?.parse().ok()?,
        load15: parts.next()?.parse().ok()?,
    })
}

pub fn parse_uptime_seconds(contents: &str) -> Option<f64> {
    contents.split_whitespace().next()?.parse().ok()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskStats {
    pub reads_completed: u64,
    pub read_bytes: u64,
    pub writes_completed: u64,
    pub written_bytes: u64,
}

const SECTOR_BYTES: u64 = 512; // kernel-internal constant, not the device's physical sector size

/// Devices named `loopN`/`ramN` are skipped — virtual devices that clutter
/// a scrape with noise no one graphs. A real hardware or LVM/RAID device
/// named similarly (unlikely, but possible) would also be skipped by this
/// simple prefix check; documented as a known simplification.
pub fn parse_diskstats(contents: &str) -> BTreeMap<String, DiskStats> {
    let mut result = BTreeMap::new();
    for line in contents.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let name = fields[2];
        if name.starts_with("loop") || name.starts_with("ram") {
            continue;
        }
        let parse = |i: usize| {
            fields
                .get(i)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        };
        result.insert(
            name.to_string(),
            DiskStats {
                reads_completed: parse(3),
                read_bytes: parse(5).saturating_mul(SECTOR_BYTES),
                writes_completed: parse(7),
                written_bytes: parse(9).saturating_mul(SECTOR_BYTES),
            },
        );
    }
    result
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetDevStats {
    pub receive_bytes: u64,
    pub transmit_bytes: u64,
}

pub fn parse_net_dev(contents: &str) -> BTreeMap<String, NetDevStats> {
    let mut result = BTreeMap::new();
    for line in contents.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let fields: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|f| f.parse().ok())
            .collect();
        if fields.len() < 16 {
            continue;
        }
        result.insert(
            name.trim().to_string(),
            NetDevStats {
                receive_bytes: fields[0],
                transmit_bytes: fields[8],
            },
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT_FIXTURE: &str = "\
cpu  10132153 290696 3084719 46828483 16683 0 25195 0 175628 0
cpu0 5132153 140696 1584719 23828483 8683 0 12595 0 87628 0
cpu1 5000000 150000 1500000 23000000 8000 0 12600 0 88000 0
intr 1234567 0 0 0
ctxt 987654321
btime 1700000000
processes 12345
";

    #[test]
    fn parses_per_core_cpu_lines_and_skips_the_aggregate() {
        let parsed = parse_stat(STAT_FIXTURE);
        assert_eq!(
            parsed.len(),
            2,
            "must not include the aggregate 'cpu ' line as a third entry"
        );
        let cpu0 = &parsed["0"];
        assert_eq!(cpu0.user, 5132153.0 / 100.0);
        assert_eq!(cpu0.idle, 23828483.0 / 100.0);
    }

    const MEMINFO_FIXTURE: &str = "\
MemTotal:       16384000 kB
MemFree:         2048000 kB
MemAvailable:    8192000 kB
Buffers:          512000 kB
Cached:          2048000 kB
SwapTotal:       4096000 kB
SwapFree:        4096000 kB
";

    #[test]
    fn meminfo_converts_kb_to_bytes() {
        let parsed = parse_meminfo(MEMINFO_FIXTURE);
        assert_eq!(parsed["MemTotal"], 16_384_000 * 1024);
        assert_eq!(parsed["MemFree"], 2_048_000 * 1024);
    }

    #[test]
    fn loadavg_parses_three_floats() {
        let parsed = parse_loadavg("0.52 0.58 0.59 2/1234 5678\n").unwrap();
        assert_eq!(
            parsed,
            LoadAvg {
                load1: 0.52,
                load5: 0.58,
                load15: 0.59
            }
        );
    }

    #[test]
    fn uptime_parses_first_field_only() {
        assert_eq!(parse_uptime_seconds("12345.67 98765.43\n"), Some(12345.67));
    }

    const DISKSTATS_FIXTURE: &str = "\
   8       0 sda 1000 5 20000 100 500 3 16000 200 0 300 300 0 0 0 0 0
   7       0 loop0 10 0 80 1 0 0 0 0 0 0 0 0 0 0 0 0
";

    #[test]
    fn diskstats_converts_sectors_to_bytes_and_skips_loop_devices() {
        let parsed = parse_diskstats(DISKSTATS_FIXTURE);
        assert_eq!(parsed.len(), 1, "loop0 must be filtered out");
        let sda = &parsed["sda"];
        assert_eq!(sda.reads_completed, 1000);
        assert_eq!(sda.read_bytes, 20000 * 512);
        assert_eq!(sda.writes_completed, 500);
        assert_eq!(sda.written_bytes, 16000 * 512);
    }

    const NETDEV_FIXTURE: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1234567    100    0    0    0     0          0         0  1234567     100    0    0    0     0       0          0
  eth0: 987654321  200    0    0    0     0          0         0 123456789   150    0    0    0     0       0          0
";

    #[test]
    fn net_dev_parses_receive_and_transmit_bytes_per_interface() {
        let parsed = parse_net_dev(NETDEV_FIXTURE);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["eth0"].receive_bytes, 987654321);
        assert_eq!(parsed["eth0"].transmit_bytes, 123456789);
        assert_eq!(parsed["lo"].receive_bytes, 1234567);
    }
}
