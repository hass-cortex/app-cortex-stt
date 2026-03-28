use clap::Parser;
use wyoming_asr::config::AppConfig;

fn main() {
    let _config = AppConfig::parse();
    println!("wyoming-asr v{}", env!("CARGO_PKG_VERSION"));
}
