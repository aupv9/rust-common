use common_error::Result;
use futures_util::future::BoxFuture;
use sqlx::{PgConnection, PgPool};

pub async fn transaction<T, F>(pool: &PgPool, f: F) -> Result<T>
where
    F: for<'c> FnOnce(&'c mut PgConnection) -> BoxFuture<'c, Result<T>>,
{
    let mut tx = pool.begin().await?;
    match f(&mut tx).await {
        Ok(value) => {
            tx.commit().await?;
            Ok(value)
        }
        Err(err) => {
            if let Err(rollback_err) = tx.rollback().await {
                tracing::warn!(error = %rollback_err, "transaction rollback failed");
            }
            Err(err)
        }
    }
}
