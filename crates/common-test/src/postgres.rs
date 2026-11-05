use common_error::{Error, ErrorKind, Result, ResultExt};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const DEFAULT_TAG: &str = "17-alpine";

pub struct TestPostgres {
    url: String,
    pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

impl TestPostgres {
    pub async fn start() -> Result<Self> {
        Self::start_with_tag(DEFAULT_TAG).await
    }

    pub async fn start_with_tag(tag: &str) -> Result<Self> {
        use testcontainers_modules::testcontainers::ImageExt;

        let container = Postgres::default()
            .with_tag(tag)
            .start()
            .await
            .map_err(|e| {
                Error::new(ErrorKind::Unavailable, "failed to start postgres container")
                    .with_source(e)
            })?;
        let host = container.get_host().await.map_err(|e| {
            Error::new(ErrorKind::Unavailable, "failed to resolve container host").with_source(e)
        })?;
        let port = container.get_host_port_ipv4(5432).await.map_err(|e| {
            Error::new(ErrorKind::Unavailable, "failed to resolve container port").with_source(e)
        })?;

        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .context(ErrorKind::Unavailable, "failed to connect to test postgres")?;

        Ok(Self {
            url,
            pool,
            _container: container,
        })
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
