use mchprs_core::server::MinecraftServer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

fn main() {
    // Setup logging
    let logfile = tracing_appender::rolling::daily("./logs", "mchprs.log");
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("MCHPRS_LOG")
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_writer(logfile.and(std::io::stdout))
        .with_env_filter(env_filter)
        .init();

    MinecraftServer::run();
}
