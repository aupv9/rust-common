use axum::Router;
use common_error::{ErrorKind, Result, ResultExt};
use common_lifecycle::{HealthRegistry, Lifecycle};
use tokio::net::TcpListener;

use crate::config::ServerConfig;
use crate::health::health_routes;
use crate::middleware::with_defaults;

pub fn app(router: Router, cfg: &ServerConfig, health: HealthRegistry) -> Result<Router> {
    Ok(with_defaults(router, cfg)?.merge(health_routes(health)))
}

pub async fn serve(router: Router, cfg: &ServerConfig, lifecycle: Lifecycle) -> Result<()> {
    let app = app(router, cfg, lifecycle.health().clone())?;
    let addr = cfg.addr();
    let listener = TcpListener::bind(addr)
        .await
        .with_context(ErrorKind::Unavailable, || format!("failed to bind {addr}"))?;
    tracing::info!(%addr, "http server listening");

    lifecycle.spawn("http-server", move |token| async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(token.cancelled_owned())
            .await
            .context(ErrorKind::Internal, "http server failed")
    });

    lifecycle.run().await
}
