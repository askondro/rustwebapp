use serde::{Deserialize, Serialize};

/// Counter state — used by the server counter page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterState {
    pub value: i32,
    pub label: String,
}

/// Generic API response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok:    bool,
    pub data:  Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

/// Employee record — mirrors the `employees` table in Postgres.
///
/// `sqlx::FromRow` — lets sqlx map a DB row directly into this struct.
///    Field names must match column names in the SQL query.
///    Only derived on non-WASM targets (server) since sqlx isn't
///    available in WASM.
///
/// `Serialize`/`Deserialize` — lets it travel as JSON between server and browser.
/// `PartialEq` — lets Leptos detect when a signal value has actually changed.
#[cfg_attr(not(target_arch = "wasm32"), derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Employee {
    pub id:         i32,
    pub name:       String,
    pub department: String,
    pub role:       String,
    pub salary:     i32,
    pub active:     bool,
}
