use clap::Parser;
use rexporter::{collect, http};

#[derive(Parser)]
#[command(
    name = "rexporter",
    about = "A lightweight Prometheus host-metrics exporter — node_exporter, but Rust"
)]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:9100")]
    listen: String,

    /// Print the current /metrics output once and exit, instead of serving.
    #[arg(long)]
    once: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.once {
        print!("{}", collect::collect_metrics_text()?);
        return Ok(());
    }

    println!(
        "rexporter listening on http://{} (GET /metrics)",
        cli.listen
    );
    http::serve(&cli.listen, collect::collect_metrics_text)
}
