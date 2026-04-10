use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use shared::{ApiResponse, CounterState, Employee};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tower_http::{compression::CompressionLayer, services::ServeDir};

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

/// All state shared across Axum request handlers.
/// `Clone` is required because Axum clones the state for each handler.
/// `Arc<Mutex<i32>>` — thread-safe shared counter (Arc = shared ownership, Mutex = safe mutation).
/// `PgPool` — sqlx connection pool; already cheaply cloneable (Arc internally).
#[derive(Clone)]
struct AppState {
    counter: Arc<Mutex<i32>>,
    db:      PgPool,
}

// ---------------------------------------------------------------------------
// Employee API handlers
// ---------------------------------------------------------------------------

/// GET /api/employees — returns all rows from the employees table.
///
/// `sqlx::query_as!` is a compile-time checked macro:
///   - verifies the SQL against the actual DB schema at compile time
///   - maps each row directly into the `Employee` struct
///   - field names in the struct must match the column names in the query
///
/// Returns `ApiResponse<Vec<Employee>>` serialized as JSON.
async fn api_get_employees(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Employee>>> {
    // `fetch_all` executes the query and collects all rows into a Vec.
    // `.await` suspends the async task until the DB responds.
    match sqlx::query_as!(
        Employee,
        "SELECT id, name, department, role, salary, active FROM employees ORDER BY id"
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(employees) => Json(ApiResponse::success(employees)),
        Err(e)        => Json(ApiResponse::error(e.to_string())),
    }
}

/// PUT /api/employees/:id — updates a single employee row.
///
/// `axum::extract::Path<i32>` extracts the `:id` segment from the URL.
/// `axum::extract::Json<Employee>` deserializes the request body.
async fn api_update_employee(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(emp): Json<Employee>,
) -> Json<ApiResponse<Employee>> {
    // `query!` (without `_as`) returns an anonymous struct with an `rows_affected` field.
    // `$1, $2, ...` are positional parameters — sqlx binds them safely (no SQL injection).
    match sqlx::query!(
        "UPDATE employees SET name=$1, department=$2, role=$3, salary=$4, active=$5 WHERE id=$6",
        emp.name, emp.department, emp.role, emp.salary, emp.active, id
    )
    .execute(&state.db)
    .await
    {
        Ok(_)  => Json(ApiResponse::success(emp)),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

/// POST /api/employees — inserts a new employee row and returns it with the DB-assigned ID.
async fn api_create_employee(
    State(state): State<AppState>,
    Json(emp): Json<Employee>,
) -> Json<ApiResponse<Employee>> {
    // `query_as!` with RETURNING lets us get the inserted row back in one query.
    // The `id` field in `emp` is ignored — Postgres generates it via SERIAL.
    match sqlx::query_as!(
        Employee,
        "INSERT INTO employees (name, department, role, salary, active)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, department, role, salary, active",
        emp.name, emp.department, emp.role, emp.salary, emp.active
    )
    .fetch_one(&state.db)
    .await
    {
        Ok(created) => Json(ApiResponse::success(created)),
        Err(e)      => Json(ApiResponse::error(e.to_string())),
    }
}

/// DELETE /api/employees/:id — removes the employee row.
async fn api_delete_employee(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Json<ApiResponse<()>> {
    match sqlx::query!("DELETE FROM employees WHERE id = $1", id)
        .execute(&state.db)
        .await
    {
        Ok(_)  => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Counter API handlers (unchanged)
// ---------------------------------------------------------------------------

async fn api_get_counter(
    State(state): State<AppState>,
) -> Json<ApiResponse<CounterState>> {
    let value = *state.counter.lock().unwrap();
    Json(ApiResponse::success(CounterState {
        value,
        label: "Server counter".into(),
    }))
}

async fn api_increment(
    State(state): State<AppState>,
) -> Json<ApiResponse<CounterState>> {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    Json(ApiResponse::success(CounterState {
        value: *counter,
        label: "Server counter".into(),
    }))
}

async fn api_decrement(
    State(state): State<AppState>,
) -> Json<ApiResponse<CounterState>> {
    let mut counter = state.counter.lock().unwrap();
    *counter -= 1;
    Json(ApiResponse::success(CounterState {
        value: *counter,
        label: "Server counter".into(),
    }))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Load .env file (DATABASE_URL etc.) into environment variables.
    // `ok()` ignores the error if .env doesn't exist — env vars set externally
    // (e.g. in production) still work.
    dotenvy::dotenv().ok();

    // Read DATABASE_URL from environment and create a connection pool.
    // `PgPool` manages multiple connections internally — sqlx reuses them
    // across requests so we don't open a new connection per query.
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env or environment");

    let db = PgPool::connect(&database_url).await
        .expect("Failed to connect to Postgres");

    println!("Connected to Postgres");

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options.clone();
    let addr = leptos_options.site_addr;
    let site_root = leptos_options.site_root.clone();

    let app_state = AppState {
        counter: Arc::new(Mutex::new(0)),
        db,
    };

    let routes = generate_route_list(app::App);

    // REST API router — all handlers share the same AppState
    let api = Router::new()
        .route("/employees",      get(api_get_employees).post(api_create_employee))
        .route("/employees/:id",  post(api_update_employee).delete(api_delete_employee))
        .route("/counter",        get(api_get_counter))
        .route("/increment",      post(api_increment))
        .route("/decrement",      post(api_decrement))
        .with_state(app_state);

    let app = Router::new()
        .nest("/api", api)
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback_service(ServeDir::new(&*site_root))
        .layer(CompressionLayer::new())
        .with_state(leptos_options);

    println!("Listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// HTML shell
// ---------------------------------------------------------------------------

fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options.clone()/>
                <leptos_meta::MetaTags/>
                <link rel="stylesheet" href="/pkg/rustwebapp.css"/>
            </head>
            <body>
                <app::App/>
            </body>
        </html>
    }
}
