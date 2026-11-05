use common_db::{DbConfig, PgPool, PoolConfig, connect, health_check, migrate, ping, transaction};
use common_error::{Error, ErrorKind};
use common_test::{TestPostgres, init_tracing};
use common_types::{Id, NonEmptyString, Secret, Timestamp, time};
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("tests/migrations");

struct User;

async fn setup() -> (TestPostgres, PgPool) {
    init_tracing();
    let pg = TestPostgres::start()
        .await
        .expect("docker is required for these tests");
    let cfg = DbConfig {
        url: Secret::new(pg.url().to_owned()),
        pool: PoolConfig {
            max_connections: 4,
            statement_timeout_ms: Some(5_000),
            ..PoolConfig::default()
        },
    };
    let pool = connect(&cfg).await.unwrap();
    migrate(&pool, &MIGRATOR).await.unwrap();
    (pg, pool)
}

async fn insert(
    conn: &mut sqlx::PgConnection,
    id: Id<User>,
    email: &str,
    name: &str,
) -> common_error::Result<()> {
    sqlx::query("INSERT INTO users (id, email, name, created_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(email)
        .bind(name)
        .bind(time::now())
        .execute(conn)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires docker"]
async fn domain_types_roundtrip_and_errors_are_mapped() {
    let (_pg, pool) = setup().await;
    ping(&pool).await.unwrap();
    health_check(pool.clone())().await.unwrap();

    let id = Id::<User>::new();
    let created = time::now();
    sqlx::query("INSERT INTO users (id, email, name, created_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("a@example.com")
        .bind(NonEmptyString::new("Alice").unwrap())
        .bind(created)
        .execute(&pool)
        .await
        .unwrap();

    let (got_id, name, got_created): (Id<User>, NonEmptyString, Timestamp) =
        sqlx::query_as("SELECT id, name, created_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((got_id, name.as_str(), got_created), (id, "Alice", created));

    let mut conn = pool.acquire().await.unwrap();
    let dup = insert(&mut conn, Id::new(), "a@example.com", "Bob")
        .await
        .unwrap_err();
    assert_eq!(dup.kind(), ErrorKind::Conflict);

    let check = insert(&mut conn, Id::new(), "c@example.com", "")
        .await
        .unwrap_err();
    assert_eq!(check.kind(), ErrorKind::InvalidInput);

    let missing: common_error::Result<(i32,)> = sqlx::query_as("SELECT 1 FROM users WHERE false")
        .fetch_one(&pool)
        .await
        .map_err(Error::from);
    assert_eq!(missing.unwrap_err().kind(), ErrorKind::NotFound);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn transaction_commits_on_ok_and_rolls_back_on_err() {
    let (_pg, pool) = setup().await;

    transaction(&pool, |tx| {
        Box::pin(async move { insert(tx, Id::new(), "ok@example.com", "Ok").await })
    })
    .await
    .unwrap();

    let err = transaction(&pool, |tx| {
        Box::pin(async move {
            insert(tx, Id::new(), "rollback@example.com", "Rb").await?;
            Err::<(), _>(Error::conflict("business rule violated"))
        })
    })
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);

    let emails: Vec<(String,)> = sqlx::query_as("SELECT email FROM users ORDER BY email")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(emails, vec![("ok@example.com".to_owned(),)]);
}
