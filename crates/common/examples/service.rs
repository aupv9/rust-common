use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use common::axum::axum::Router;
use common::axum::axum::extract::State;
use common::axum::axum::http::StatusCode;
use common::axum::axum::routing::get;
use common::axum::{ApiResult, Json, Path, Query, ServerConfig, ValidatedJson};
use common::config::ConfigLoader;
use common::lifecycle::{Lifecycle, LifecycleConfig};
use common::telemetry::{self, ServiceInfo, TelemetryConfig, tracing};
use common::types::{Id, NonEmptyString, Page, PageRequest, Timestamp, Validate, time};
use common::{Error, OptionExt};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AppConfig {
    server: ServerConfig,
    telemetry: TelemetryConfig,
    lifecycle: LifecycleConfig,
}

type UserId = Id<User>;
type Store = Arc<RwLock<BTreeMap<UserId, User>>>;

#[derive(Debug, Clone, Serialize)]
struct User {
    id: UserId,
    name: NonEmptyString,
    created_at: Timestamp,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: NonEmptyString,
}

impl Validate for CreateUser {
    fn validate(&self) -> common::Result<()> {
        if self.name.chars().count() > 50 {
            return Err(Error::invalid_input("name must be at most 50 characters"));
        }
        Ok(())
    }
}

async fn create_user(
    State(store): State<Store>,
    ValidatedJson(input): ValidatedJson<CreateUser>,
) -> (StatusCode, Json<User>) {
    let user = User {
        id: Id::new(),
        name: input.name,
        created_at: time::now(),
    };
    store
        .write()
        .expect("store lock")
        .insert(user.id, user.clone());
    (StatusCode::CREATED, Json(user))
}

async fn get_user(State(store): State<Store>, Path(id): Path<UserId>) -> ApiResult<Json<User>> {
    let user = store
        .read()
        .expect("store lock")
        .get(&id)
        .cloned()
        .or_not_found(format!("user {id}"))?;
    Ok(Json(user))
}

async fn list_users(
    State(store): State<Store>,
    Query(page): Query<PageRequest>,
) -> Json<Page<User>> {
    let store = store.read().expect("store lock");
    let offset = usize::try_from(page.offset()).unwrap_or(usize::MAX);
    let limit = usize::try_from(page.limit()).unwrap_or(usize::MAX);
    let items = store.values().skip(offset).take(limit).cloned().collect();
    Json(Page::new(items, page, store.len() as u64))
}

#[tokio::main]
async fn main() -> common::Result<()> {
    let cfg: AppConfig = ConfigLoader::from_env("APP").load()?;
    let service = ServiceInfo::new("example-service", env!("CARGO_PKG_VERSION"));
    let _telemetry = telemetry::init(&service, &cfg.telemetry)?;

    let store = Store::default();
    let lifecycle = Lifecycle::new(&cfg.lifecycle);

    let probe = Arc::clone(&store);
    lifecycle.health().register("store", move || {
        let ok = !probe.is_poisoned();
        async move {
            if ok {
                Ok(())
            } else {
                Err(Error::internal("store lock poisoned"))
            }
        }
    });

    lifecycle.spawn("stats-reporter", {
        let store = Arc::clone(&store);
        move |token| async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    () = token.cancelled() => break,
                    _ = interval.tick() => {
                        let users = store.read().map(|s| s.len()).unwrap_or_default();
                        tracing::info!(users, "stats");
                    }
                }
            }
            Ok(())
        }
    });

    let router = Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user))
        .with_state(store);

    common::axum::serve(router, &cfg.server, lifecycle).await
}
