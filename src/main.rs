use clap::Parser;
use wyoming_asr::config::AppConfig;

fn main() {
    let config = AppConfig::parse();
    println!("wyoming-asr v{}", env!("CARGO_PKG_VERSION"));
    println!("Wyoming: {}:{}", config.wyoming_host, config.wyoming_port);
    println!("HTTP: {}:{}", config.http_host, config.http_port);
    println!("Data dir: {:?}", config.data_dir);
    println!("Default model: {}", config.default_model);
}
