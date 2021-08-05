use std::path::PathBuf;

use clap::{AppSettings, Clap};

use stargazer_lib::Config;

mod app;

#[derive(Clap)]
#[clap(
    version = "1.0",
    author = "LightQuantum <self@lightquantum.me> and George Miao <gm@miao.dev>"
)]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Sets a custom config file. This flag overrides system-wide and user-wide configs.
    #[clap(short, long)]
    config: Option<PathBuf>,
}

fn main() {
    let opts = Opts::parse();
    let config = Config::new(opts.config.as_deref());
    println!("{:?}", config);
}
