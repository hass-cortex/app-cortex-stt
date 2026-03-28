use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "wyoming-asr")]
pub struct AppConfig {
    #[arg(long, env = "WYOMING_PORT", default_value_t = 10300)]
    pub wyoming_port: u16,
}
