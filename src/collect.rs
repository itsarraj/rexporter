use std::fs;

use anyhow::Context;

use crate::metrics;
use crate::render;

/// Reads the real `/proc` files and renders the current Prometheus text.
/// The only non-pure function in this crate — everything it calls
/// (`metrics::parse_*`, `render::render`) is a pure function tested with
/// fixtures in its own module.
pub fn collect_metrics_text() -> anyhow::Result<String> {
    let stat = fs::read_to_string("/proc/stat").context("reading /proc/stat")?;
    let meminfo = fs::read_to_string("/proc/meminfo").context("reading /proc/meminfo")?;
    let loadavg = fs::read_to_string("/proc/loadavg").context("reading /proc/loadavg")?;
    let uptime = fs::read_to_string("/proc/uptime").context("reading /proc/uptime")?;
    let diskstats = fs::read_to_string("/proc/diskstats").context("reading /proc/diskstats")?;
    let netdev = fs::read_to_string("/proc/net/dev").context("reading /proc/net/dev")?;

    let cpu = metrics::parse_stat(&stat);
    let mem = metrics::parse_meminfo(&meminfo);
    let load = metrics::parse_loadavg(&loadavg);
    let up = metrics::parse_uptime_seconds(&uptime);
    let disk = metrics::parse_diskstats(&diskstats);
    let net = metrics::parse_net_dev(&netdev);

    Ok(render::render(&cpu, &mem, load, up, &disk, &net))
}
