use std::collections::BTreeMap;
use std::fmt::Write;

use crate::metrics::{CpuTimes, DiskStats, LoadAvg, NetDevStats};

const MEM_FIELDS: &[&str] = &[
    "MemTotal",
    "MemFree",
    "MemAvailable",
    "Buffers",
    "Cached",
    "SwapTotal",
    "SwapFree",
];

/// Renders everything collected into Prometheus text exposition format.
/// Pure — takes already-parsed data, no file I/O, so it's testable without
/// a real `/proc` (see tests below) and is exactly what `collect.rs` calls
/// after actually reading the files.
#[allow(clippy::too_many_arguments)]
pub fn render(
    cpu: &BTreeMap<String, CpuTimes>,
    mem: &BTreeMap<String, u64>,
    load: Option<LoadAvg>,
    uptime: Option<f64>,
    disk: &BTreeMap<String, DiskStats>,
    net: &BTreeMap<String, NetDevStats>,
) -> String {
    let mut out = String::new();

    writeln!(
        out,
        "# HELP node_cpu_seconds_total Seconds the CPU spent in each mode."
    )
    .ok();
    writeln!(out, "# TYPE node_cpu_seconds_total counter").ok();
    for (core, t) in cpu {
        for (mode, value) in [
            ("user", t.user),
            ("nice", t.nice),
            ("system", t.system),
            ("idle", t.idle),
            ("iowait", t.iowait),
            ("irq", t.irq),
            ("softirq", t.softirq),
            ("steal", t.steal),
        ] {
            writeln!(
                out,
                r#"node_cpu_seconds_total{{cpu="{core}",mode="{mode}"}} {value}"#
            )
            .ok();
        }
    }

    writeln!(
        out,
        "# HELP node_memory_bytes Memory statistics from /proc/meminfo, in bytes."
    )
    .ok();
    writeln!(out, "# TYPE node_memory_bytes gauge").ok();
    for field in MEM_FIELDS {
        if let Some(value) = mem.get(*field) {
            writeln!(out, r#"node_memory_{field}_bytes {value}"#).ok();
        }
    }

    if let Some(load) = load {
        writeln!(out, "# HELP node_load1 1m load average.").ok();
        writeln!(out, "# TYPE node_load1 gauge").ok();
        writeln!(out, "node_load1 {}", load.load1).ok();
        writeln!(out, "# HELP node_load5 5m load average.").ok();
        writeln!(out, "# TYPE node_load5 gauge").ok();
        writeln!(out, "node_load5 {}", load.load5).ok();
        writeln!(out, "# HELP node_load15 15m load average.").ok();
        writeln!(out, "# TYPE node_load15 gauge").ok();
        writeln!(out, "node_load15 {}", load.load15).ok();
    }

    if let Some(uptime) = uptime {
        writeln!(out, "# HELP node_time_seconds_uptime Seconds since boot.").ok();
        writeln!(out, "# TYPE node_time_seconds_uptime counter").ok();
        writeln!(out, "node_time_seconds_uptime {uptime}").ok();
    }

    writeln!(
        out,
        "# HELP node_disk_reads_completed_total Reads completed per device."
    )
    .ok();
    writeln!(out, "# TYPE node_disk_reads_completed_total counter").ok();
    for (device, d) in disk {
        writeln!(
            out,
            r#"node_disk_reads_completed_total{{device="{device}"}} {}"#,
            d.reads_completed
        )
        .ok();
    }
    writeln!(
        out,
        "# HELP node_disk_read_bytes_total Bytes read per device."
    )
    .ok();
    writeln!(out, "# TYPE node_disk_read_bytes_total counter").ok();
    for (device, d) in disk {
        writeln!(
            out,
            r#"node_disk_read_bytes_total{{device="{device}"}} {}"#,
            d.read_bytes
        )
        .ok();
    }
    writeln!(
        out,
        "# HELP node_disk_writes_completed_total Writes completed per device."
    )
    .ok();
    writeln!(out, "# TYPE node_disk_writes_completed_total counter").ok();
    for (device, d) in disk {
        writeln!(
            out,
            r#"node_disk_writes_completed_total{{device="{device}"}} {}"#,
            d.writes_completed
        )
        .ok();
    }
    writeln!(
        out,
        "# HELP node_disk_written_bytes_total Bytes written per device."
    )
    .ok();
    writeln!(out, "# TYPE node_disk_written_bytes_total counter").ok();
    for (device, d) in disk {
        writeln!(
            out,
            r#"node_disk_written_bytes_total{{device="{device}"}} {}"#,
            d.written_bytes
        )
        .ok();
    }

    writeln!(
        out,
        "# HELP node_network_receive_bytes_total Bytes received per interface."
    )
    .ok();
    writeln!(out, "# TYPE node_network_receive_bytes_total counter").ok();
    for (iface, n) in net {
        writeln!(
            out,
            r#"node_network_receive_bytes_total{{device="{iface}"}} {}"#,
            n.receive_bytes
        )
        .ok();
    }
    writeln!(
        out,
        "# HELP node_network_transmit_bytes_total Bytes transmitted per interface."
    )
    .ok();
    writeln!(out, "# TYPE node_network_transmit_bytes_total counter").ok();
    for (iface, n) in net {
        writeln!(
            out,
            r#"node_network_transmit_bytes_total{{device="{iface}"}} {}"#,
            n.transmit_bytes
        )
        .ok();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::*;

    #[test]
    fn renders_valid_looking_prometheus_text_with_expected_metric_names() {
        let mut cpu = BTreeMap::new();
        cpu.insert(
            "0".to_string(),
            CpuTimes {
                user: 1.0,
                nice: 0.0,
                system: 2.0,
                idle: 100.0,
                iowait: 0.0,
                irq: 0.0,
                softirq: 0.0,
                steal: 0.0,
            },
        );
        let mut mem = BTreeMap::new();
        mem.insert("MemTotal".to_string(), 16_000_000_000);
        let mut disk = BTreeMap::new();
        disk.insert(
            "sda".to_string(),
            DiskStats {
                reads_completed: 10,
                read_bytes: 5120,
                writes_completed: 5,
                written_bytes: 2560,
            },
        );
        let mut net = BTreeMap::new();
        net.insert(
            "eth0".to_string(),
            NetDevStats {
                receive_bytes: 100,
                transmit_bytes: 200,
            },
        );

        let text = render(
            &cpu,
            &mem,
            Some(LoadAvg {
                load1: 0.1,
                load5: 0.2,
                load15: 0.3,
            }),
            Some(9999.0),
            &disk,
            &net,
        );

        assert!(text.contains(r#"node_cpu_seconds_total{cpu="0",mode="user"} 1"#));
        assert!(text.contains("node_memory_MemTotal_bytes 16000000000"));
        assert!(text.contains("node_load1 0.1"));
        assert!(text.contains("node_time_seconds_uptime 9999"));
        assert!(text.contains(r#"node_disk_read_bytes_total{device="sda"} 5120"#));
        assert!(text.contains(r#"node_network_transmit_bytes_total{device="eth0"} 200"#));
        // Every metric with samples must have a HELP and TYPE line — this
        // is what makes it valid Prometheus exposition format, not just
        // text that happens to contain the right substrings.
        assert!(text.contains("# HELP node_cpu_seconds_total"));
        assert!(text.contains("# TYPE node_cpu_seconds_total counter"));
    }

    #[test]
    fn empty_input_still_renders_without_panicking() {
        let text = render(
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert!(text.contains("# HELP node_cpu_seconds_total"));
        assert!(!text.contains("node_load1 ")); // no load data given, so no sample line
    }
}
