use tracing_subscriber::{
    Layer,
    filter::{EnvFilter, filter_fn},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

/// An error logged by this crate itself. These go to the pretty layer for a
/// readable multi-line render; everything else takes the compact layer.
fn is_crate_error(meta: &tracing::Metadata<'_>, crate_name: &str) -> bool {
    meta.target().starts_with(crate_name) && *meta.level() == tracing::Level::ERROR
}

pub fn init() {
    let crate_name = env!("CARGO_CRATE_NAME");

    let global_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("{}=debug,tower_http=debug", crate_name).into());

    let pretty_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_writer(std::io::stderr)
        .with_filter(filter_fn(move |meta| is_crate_error(meta, crate_name)));

    let compact_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(std::io::stderr)
        .with_filter(filter_fn(move |meta| !is_crate_error(meta, crate_name)));

    tracing_subscriber::registry()
        .with(global_filter)
        .with(pretty_layer)
        .with(compact_layer)
        .init();
}
