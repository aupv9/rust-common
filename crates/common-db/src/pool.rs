use std::str::FromStr;
use std::time::Duration;

use common_error::{Error, ErrorKind, Result};
use futures_util::future::BoxFuture;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};

use crate::config::DbConfig;

pub async fn connect(cfg: &DbConfig) -> Result<PgPool> {
    let pool = pool_options(cfg)
        .connect_with(connect_options(cfg)?)
        .await?;
    tracing::info!(
        max_connections = cfg.pool.max_connections,
        "database pool connected"
    );
    Ok(pool)
}

pub fn connect_lazy(cfg: &DbConfig) -> Result<PgPool> {
    Ok(pool_options(cfg).connect_lazy_with(connect_options(cfg)?))
}

pub async fn ping(pool: &PgPool) -> Result<()> {
    pool.execute("SELECT 1").await?;
    Ok(())
}

pub fn health_check(
    pool: PgPool,
) -> impl Fn() -> BoxFuture<'static, Result<()>> + Send + Sync + 'static {
    move || {
        let pool = pool.clone();
        Box::pin(async move { ping(&pool).await })
    }
}

pub async fn migrate(pool: &PgPool, migrator: &Migrator) -> Result<()> {
    migrator
        .run(pool)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, "database migration failed").with_source(e))?;
    tracing::info!(
        migrations = migrator.iter().count(),
        "database migrations applied"
    );
    Ok(())
}

fn connect_options(cfg: &DbConfig) -> Result<PgConnectOptions> {
    let mut options = PgConnectOptions::from_str(cfg.url.expose())
        .map_err(|e| Error::new(ErrorKind::Config, "invalid database url").with_source(e))?;
    if let Some(name) = &cfg.pool.application_name {
        options = options.application_name(name);
    }
    if let Some(ms) = cfg.pool.statement_timeout_ms {
        options = options.options([("statement_timeout", format!("{ms}ms"))]);
    }
    Ok(options)
}

fn pool_options(cfg: &DbConfig) -> PgPoolOptions {
    let pool = &cfg.pool;
    PgPoolOptions::new()
        .max_connections(pool.max_connections)
        .min_connections(pool.min_connections)
        .acquire_timeout(Duration::from_secs(pool.acquire_timeout_secs))
        .idle_timeout(pool.idle_timeout_secs.map(Duration::from_secs))
        .max_lifetime(pool.max_lifetime_secs.map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use common_types::Secret;

    use super::*;
    use crate::config::PoolConfig;

    #[test]
    fn invalid_url_is_config_error() {
        let cfg = DbConfig {
            url: Secret::new("not a url".to_owned()),
            pool: PoolConfig::default(),
        };
        assert_eq!(connect_lazy(&cfg).unwrap_err().kind(), ErrorKind::Config);
    }
}
