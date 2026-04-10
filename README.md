# rustwebapp

Full-stack Rust — **Leptos** (frontend/WASM) + **Axum** (backend).

## Structure

```
rustwebapp/
├── Cargo.toml          ← workspace root
├── shared/             ← types shared by frontend + backend
│   └── src/lib.rs
├── app/                ← Leptos frontend (compiles to WASM)
│   ├── Cargo.toml
│   └── src/lib.rs
├── server/             ← Axum backend (SSR + REST API)
│   ├── Cargo.toml
│   └── src/main.rs
└── style/
    └── main.css
```

## Prerequisites

```bash
# Rust nightly (Leptos 0.7 uses nightly features)
rustup toolchain install nightly
rustup default nightly

# WASM target
rustup target add wasm32-unknown-unknown

# cargo-leptos build tool
cargo install cargo-leptos --locked
```

## Development

```bash
# Run with hot-reload (recompiles on file change)
cargo leptos watch
```

Open http://127.0.0.1:3000

## Production build

```bash
cargo leptos build --release
# Binary: target/server/release/server
# Assets: target/site/
```

## How it works

```
Browser
  │
  ├── GET /          → Axum serves SSR HTML (Leptos renders on server)
  ├── GET /pkg/*.wasm → WASM bundle (Leptos hydrates in browser)
  │
  ├── GET  /api/counter   → JSON { ok, data: { value, label } }
  ├── POST /api/increment → increments server-side counter
  └── POST /api/decrement → decrements server-side counter

shared crate
  └── CounterState, ApiResponse<T>
      used by both app/ and server/ — same types, no drift
```

## Adding a new page

1. Add a `#[component]` in `app/src/lib.rs`
2. Add a `<Route path="/new" view=NewPage/>` in `App`
3. Add a `<A href="/new">` in the nav

## Adding a new API endpoint

1. Add handler in `server/src/main.rs`
2. Register: `.route("/api/new", get(my_handler))`
3. If it returns data — add the type to `shared/src/lib.rs`
# rustwebapp
# rustwebapp
