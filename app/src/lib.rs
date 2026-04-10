//! app/src/lib.rs
//!
//! This is the frontend crate. It compiles in two modes:
//!
//! 1. `--features hydrate` → target `wasm32-unknown-unknown`
//!    Produces `rustwebapp.wasm` that the browser downloads and runs.
//!    The `hydrate()` function is the WASM entry point.
//!
//! 2. `--features ssr` → target `x86_64` (native)
//!    Used by the server crate to import `App` for server-side rendering.
//!    All `#[cfg(target_arch = "wasm32")]` blocks are compiled out here.

// `leptos::prelude::*` brings in the most commonly used items:
//   signal, RwSignal, Effect, view!, #[component], Callback,
//   event_target_value, event_target_checked, etc.
use leptos::prelude::*;

// Client-side router components.
// `Router`  — must wrap everything that uses routing.
// `Routes`  — renders the matched page component.
// `Route`   — maps a path pattern to a component.
// `A`       — like <a> but does client-side navigation + adds "active" class.
// `path!`   — macro that creates a typed path segment (safer than raw strings).
use leptos_router::{
    components::{A, Route, Router, Routes},
    path,
};

// Types from our own `shared` crate — compiled into both this WASM binary
// and the server binary. Using the same Rust types on both sides means the
// JSON shape can never drift between client and server.
use shared::{ApiResponse, CounterState, Employee};

// `JsCast` is a wasm_bindgen trait that provides `dyn_into` and
// `unchecked_ref` — used to cast generic JS objects to specific types.
// Only compiled for WASM because it has no meaning on the server.
#[cfg(target_arch = "wasm32")]
use leptos::wasm_bindgen::JsCast;

// ─────────────────────────────────────────────────────────────────────────────
// Root app component
// ─────────────────────────────────────────────────────────────────────────────

/// `App` is the root component — the single entry point for the entire UI.
///
/// It owns two pieces of global state:
///   - `dark`  — whether night theme is active
///   - `clock` — the current time string, updated every second
///
/// Everything else (routing, pages, sidebar) is rendered inside here.
///
/// `#[component]` is a proc macro that transforms this plain Rust function
/// into a Leptos component. The return type `impl IntoView` means "anything
/// that can be rendered to the DOM".
#[component]
pub fn App() -> impl IntoView {

    // ── Theme initialisation ──────────────────────────────────────────────────
    //
    // We want the theme choice to survive a page refresh.
    // Strategy: read `localStorage["theme"]` before creating the signal so
    // the very first render already has the right value.
    //
    // `#[allow(unused_mut)]` suppresses a warning on the server build where
    // `initial_dark` is never mutated (the cfg block below is compiled out).

    #[allow(unused_mut)]
    let mut initial_dark = false; // default: day mode

    // This entire block is compiled out in the SSR (server) build.
    // On the server there is no browser, no window, no localStorage.
    #[cfg(target_arch = "wasm32")]
    {
        // `leptos::web_sys::window()` returns `Option<Window>` — None if called
        // outside a browser context. `.and_then` chains optional operations.
        // `local_storage()` returns `Result<Option<Storage>, JsValue>`.
        // `.ok().flatten()` collapses Result<Option<_>> → Option<_>.
        if let Some(storage) = leptos::web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
        {
            // `get_item` returns `Result<Option<String>, JsValue>`.
            // If the key exists and its value is "dark", start in dark mode.
            if let Ok(Some(val)) = storage.get_item("theme") {
                initial_dark = val == "dark";
            }
        }
    }

    // `signal(value)` creates a reactive signal — a read/write pair.
    //   `dark`     — ReadSignal<bool>: call `.get()` inside a closure to subscribe
    //   `set_dark` — WriteSignal<bool>: call `.set(val)` or `.update(|v| ...)` to change
    //
    // Any closure that reads `dark.get()` will re-run automatically when
    // the value changes. This is fine-grained reactivity — no virtual DOM.
    let (dark, set_dark) = signal(initial_dark);

    // ── Theme effect ──────────────────────────────────────────────────────────
    //
    // `Effect::new` runs its closure after the component renders, and re-runs
    // it whenever any signal it reads changes. Here it reads `dark`, so it
    // fires once on mount and again every time the user toggles the theme.
    //
    // We need to touch `document.body` which is outside the Leptos component
    // tree, so we do it via direct web_sys DOM manipulation instead of
    // reactive attributes.
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let is_dark = dark.get(); // subscribe: effect re-runs when dark changes
            let win = leptos::web_sys::window().unwrap();

            // Toggle the "theme-light" class on <body>.
            // All CSS variables are defined on :root but overridden by
            // body.theme-light — so toggling this class repaints everything.
            if let Some(body) = win.document().and_then(|d| d.body()) {
                if is_dark {
                    // Night mode: remove the light-theme override
                    let _ = body.class_list().remove_1("theme-light");
                } else {
                    // Day mode: add the light-theme override
                    let _ = body.class_list().add_1("theme-light");
                }
            }

            // Persist the choice so it survives a page refresh.
            // `set_item(key, value)` returns Result — we ignore errors with `let _`.
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.set_item(
                    "theme",
                    if is_dark { "dark" } else { "light" },
                );
            }
        }
    });

    // ── Clock ─────────────────────────────────────────────────────────────────
    //
    // `clock` holds the current time as a formatted string "HH:MM:SS".
    // It starts empty — on the server it stays empty (no `set_interval`).
    // On the client the Effect below sets up a 1-second browser timer.
    let (clock, set_clock) = signal(String::new());

    Effect::new(move |_| {
        // This entire block is compiled out in the SSR build.
        // `set_interval` requires a browser — calling it on the server
        // would panic.
        #[cfg(target_arch = "wasm32")]
        {
            use leptos::web_sys::window;

            // Closure that reads the current time and updates the signal.
            // `js_sys::Date::new_0()` calls `new Date()` in JavaScript.
            // `.get_hours()`, `.get_minutes()`, `.get_seconds()` are JS Date methods.
            // `{:02}` formats with leading zero so "9" becomes "09".
            let update_clock = move || {
                let now = js_sys::Date::new_0();
                set_clock.set(format!(
                    "{:02}:{:02}:{:02}",
                    now.get_hours(),   // local hours   0-23
                    now.get_minutes(), // local minutes  0-59
                    now.get_seconds(), // local seconds  0-59
                ));
            };

            // Call immediately so the clock shows on first render
            // rather than waiting a full second.
            update_clock();

            // Wrap the closure in a `wasm_bindgen::closure::Closure` so it
            // can be passed to JavaScript's `setInterval`.
            // `Box<dyn Fn()>` is a heap-allocated type-erased function pointer.
            let cb = wasm_bindgen::closure::Closure::wrap(
                Box::new(update_clock) as Box<dyn Fn()>
            );

            // `set_interval_with_callback_and_timeout_and_arguments_0` is the
            // web_sys binding for `window.setInterval(fn, ms)`.
            // `cb.as_ref().unchecked_ref()` converts the Rust closure to a
            // raw JS `Function` reference that the browser can call.
            window()
                .unwrap()
                .set_interval_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    1000, // interval in milliseconds
                )
                .unwrap();

            // `cb.forget()` intentionally leaks the closure.
            // Normally Rust would drop it at end of scope, making the browser
            // call a freed function pointer. Since this timer runs for the
            // entire app lifetime, leaking is the correct pattern here.
            cb.forget();
        }
    });

    // ── View ──────────────────────────────────────────────────────────────────
    //
    // `view!` is a macro that accepts JSX-like syntax and expands to Rust code
    // that builds a reactive DOM tree. String literals need explicit quotes.
    // Rust expressions go inside `{}`.
    view! {
        // `<Router>` must wrap everything that uses routing — it provides
        // the routing context that `<Routes>`, `<A>`, and `use_navigate` need.
        <Router>
            <div class="layout">

                // ── Sidebar ───────────────────────────────────────────────
                <aside class="sidebar">
// sidebar-header intentionally left empty — logo/title pending
                    <nav class="sidebar-nav">
                        // `<A>` renders as a normal <a> tag but intercepts clicks
                        // for client-side navigation (no full page reload).
                        // It also adds class="active" when its href matches the
                        // current URL — used by CSS for the highlighted menu item.
                        <A href="/">"🏠 Home"</A>
                        <A href="/counter">"🔢 Counter"</A>
                        <A href="/local">"➕ Local Counter"</A>
                        <A href="/data">"📊 Data"</A>
                        <A href="/about">"ℹ️ About"</A>
                    </nav>
                </aside>

                // ── Main area ─────────────────────────────────────────────
                <div class="main-area">
                    <header class="topbar">
                        <div class="topbar-left">
                            <h1>"Demo GUI"</h1>
                            <p class="subtitle">"Leptos + Axum · v0.1.0"</p>
                        </div>
                        <div class="topbar-right">
                            // `{move || clock.get()}` is a reactive closure.
                            // Leptos tracks that it reads `clock` and re-runs
                            // only this tiny span of DOM when clock changes —
                            // not the whole component. This is fine-grained
                            // reactivity: no virtual DOM diffing needed.
                            <span class="topbar-clock">{move || clock.get()}</span>

                            // Theme toggle button.
                            // `on:click` attaches a DOM event listener.
                            // `.update(|d| *d = !*d)` flips the bool in-place.
                            // The button label is itself a reactive closure that
                            // re-renders when `dark` changes.
                            <button class="theme-toggle"
                                on:click=move |_| set_dark.update(|d| *d = !*d)>
                                {move || if dark.get() { "☀ Day" } else { "🌙 Night" }}
                            </button>
                        </div>
                    </header>

                    <main class="content">
                        // `<Routes>` renders whichever `<Route>` matches the
                        // current URL. `fallback` is shown when nothing matches.
                        // Routes are matched in order — first match wins.
                        <Routes fallback=|| view! { <NotFound/> }>
                            <Route path=path!("/")        view=HomePage/>
                            <Route path=path!("/counter") view=CounterPage/>
                            <Route path=path!("/local")   view=LocalCounterPage/>
                            <Route path=path!("/data")    view=DataPage/>
                            <Route path=path!("/about")   view=AboutPage/>
                        </Routes>
                    </main>

                    <footer class="footer">
                        <p>"Built with Rust 🦀 — Leptos + Axum"</p>
                    </footer>
                </div>
            </div>
        </Router>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Simple pages
// ─────────────────────────────────────────────────────────────────────────────

/// Shown when no route matches (the router's fallback).
#[component]
fn NotFound() -> impl IntoView {
    view! { <h2>"404 — Page not found"</h2> }
}

/// Landing page — static content, no signals needed.
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <section class="page">
            <h2>"Welcome"</h2>
            <p>
                "This is a full-stack Rust web application. "
                "The backend is "<strong>"Axum"</strong>
                " and the frontend is "<strong>"Leptos"</strong>
                " compiled to WebAssembly."
            </p>
            <ul>
                <li>"Type-safe across client and server via the "<code>"shared"</code>" crate"</li>
                <li>"Reactive UI with fine-grained signals (no Virtual DOM)"</li>
                <li>"Server-side rendering + client-side hydration"</li>
            </ul>
        </section>
    }
}

/// About page — lists the technology stack.
#[component]
fn AboutPage() -> impl IntoView {
    view! {
        <section class="page">
            <h2>"About"</h2>
            <p>"Stack:"</p>
            <ul>
                <li><code>"axum 0.7"</code>" — async HTTP server"</li>
                <li><code>"leptos 0.7"</code>" — reactive frontend (WASM)"</li>
                <li><code>"tokio"</code>" — async runtime"</li>
                <li><code>"serde"</code>" — serialization"</li>
                <li><code>"shared"</code>" — types visible to both sides"</li>
            </ul>
        </section>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Data types for the employee table
// ─────────────────────────────────────────────────────────────────────────────

// Employee type is imported from the shared crate.

/// Which column the table is currently sorted by.
/// `PartialEq` lets us compare columns in the sort/arrow logic.
#[derive(Clone, PartialEq)]
enum SortCol { Id, Name, Department, Salary }

// ─────────────────────────────────────────────────────────────────────────────
// Edit dialog
// ─────────────────────────────────────────────────────────────────────────────

/// Modal dialog for editing a single employee record.
///
/// Props:
///   `employee` — a read signal holding the employee being edited.
///                `ReadSignal<Option<Employee>>`: None = dialog should be closed.
///   `on_save`  — callback invoked with the modified Employee when user clicks Save.
///   `on_close` — callback invoked with () when user cancels or clicks outside.
///
/// `Callback<T>` is Leptos's typed callback type. The parent passes these in
/// so the dialog can communicate changes upward without the parent needing to
/// pass writable signals down (which would create tight coupling).
#[component]
fn EditDialog(
    employee: ReadSignal<Option<Employee>>,
    on_save:  Callback<Employee>,
    on_close: Callback<()>,
) -> impl IntoView {

    // Each field gets its own `RwSignal` — a single handle that can both
    // read and write (unlike `signal()` which returns separate read/write handles).
    // These are the live editable values shown in the form inputs.
    let name       = RwSignal::new(String::new());
    let department = RwSignal::new(String::new());
    let role       = RwSignal::new(String::new());
    let salary     = RwSignal::new(0i32);
    let active     = RwSignal::new(true);

    // Sync the form fields whenever the `employee` signal changes.
    // This runs once when the dialog first opens (employee goes from None → Some),
    // and again if a different employee is selected while the dialog is open.
    Effect::new(move |_| {
        if let Some(e) = employee.get() {
            // `.clone()` is needed because `e.name` is a String (not Copy)
            // and `name.set()` takes ownership of the value.
            name.set(e.name.clone());
            department.set(e.department.clone());
            role.set(e.role.clone());
            salary.set(e.salary);
            active.set(e.active);
        }
    });

    // Closure called when user clicks Save.
    // Reads the current signal values, constructs a new Employee, and
    // fires the `on_save` callback so the parent can update its own state.
    let handle_save = move |_| {
        if let Some(e) = employee.get() {
            // `.run(value)` invokes the Callback — equivalent to calling the closure.
            on_save.run(Employee {
                id: e.id,               // preserve the original ID
                name:       name.get(),
                department: department.get(),
                role:       role.get(),
                salary:     salary.get(),
                active:     active.get(),
            });
        }
    };

    view! {
        // Semi-transparent backdrop covers the whole screen.
        // Clicking it fires on_close so the user can dismiss by clicking outside.
        <div class="dialog-backdrop" on:click=move |_| on_close.run(())>

            // The dialog box itself.
            // `e.stop_propagation()` prevents the click from bubbling up to
            // the backdrop, which would immediately close the dialog.
            <div class="dialog" on:click=move |e| e.stop_propagation()>

                <div class="dialog-header">
                    <h3>"Edit Employee"</h3>
                    // ✕ close button in the top-right corner
                    <button class="dialog-close" on:click=move |_| on_close.run(())>"✕"</button>
                </div>

                <div class="dialog-body">

                    // Name field
                    // `prop:value` sets the DOM property (not attribute) reactively.
                    // Using `prop:` instead of `attr:` ensures the input always
                    // shows the current signal value even after user edits.
                    // `on:input` fires on every keystroke and updates the signal.
                    // `event_target_value(&e)` is a Leptos helper that reads
                    // the input's current string value from the event.
                    <label class="field">
                        <span>"Name"</span>
                        <input type="text"
                            prop:value=move || name.get()
                            on:input=move |e| name.set(event_target_value(&e))
                        />
                    </label>

                    // Department dropdown
                    // `on:change` fires when the selected option changes.
                    // Each `<option>` has `selected=move || ...` which reactively
                    // marks the correct option as selected when the signal matches.
                    <label class="field">
                        <span>"Department"</span>
                        <select
                            prop:value=move || department.get()
                            on:change=move |e| department.set(event_target_value(&e))>
                            <option value="Engineering" selected=move || department.get() == "Engineering">"Engineering"</option>
                            <option value="Design"      selected=move || department.get() == "Design"     >"Design"</option>
                            <option value="Sales"       selected=move || department.get() == "Sales"      >"Sales"</option>
                            <option value="HR"          selected=move || department.get() == "HR"         >"HR"</option>
                        </select>
                    </label>

                    // Role text field — same pattern as Name
                    <label class="field">
                        <span>"Role"</span>
                        <input type="text"
                            prop:value=move || role.get()
                            on:input=move |e| role.set(event_target_value(&e))
                        />
                    </label>

                    // Salary number field
                    // `.parse::<i32>()` converts the string from the input to u32.
                    // We only update the signal if parsing succeeds — invalid input
                    // (e.g. letters) is silently ignored.
                    <label class="field">
                        <span>"Salary"</span>
                        <input type="number"
                            prop:value=move || salary.get().to_string()
                            on:input=move |e| {
                                if let Ok(v) = event_target_value(&e).parse::<i32>() {
                                    salary.set(v);
                                }
                            }
                        />
                    </label>

                    // Active checkbox
                    // `prop:checked` sets the checked state reactively.
                    // `event_target_checked` is a Leptos helper that reads the
                    // checkbox's boolean checked state from the event — no need
                    // to cast to HtmlInputElement manually.
                    <label class="field field-check">
                        <input type="checkbox"
                            prop:checked=move || active.get()
                            on:change=move |e| active.set(event_target_checked(&e))
                        />
                        <span>"Active"</span>
                    </label>
                </div>

                <div class="dialog-footer">
                    <button class="btn-cancel" on:click=move |_| on_close.run(())>"Cancel"</button>
                    <button class="btn-save"   on:click=handle_save>"Save"</button>
                </div>
            </div>
        </div>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Data page — sortable, filterable employee table
// ─────────────────────────────────────────────────────────────────────────────

/// The main data page.
///
/// Owns all table state: the full row list, sort column/direction, filter string,
/// and which employee (if any) is currently open in the edit dialog.
///
/// All state is held in signals — changing any signal automatically re-renders
/// only the parts of the DOM that depend on it.
#[component]
fn DataPage() -> impl IntoView {

    // Rows start empty — fetched from the server on mount.
    let rows = RwSignal::new(Vec::<Employee>::new());
    // Loading and error state for the initial fetch
    let (loading, set_loading) = signal(true);
    let (fetch_error, set_fetch_error) = signal(Option::<String>::None);

    // Fetch employees from the server when the component mounts.
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            match fetch_employees().await {
                Ok(data) => {
                    rows.set(data);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_fetch_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    // Sort state: which column and which direction (ascending = true).
    // Using `signal()` (two-handle) because read and write happen in separate
    // closures that each only need one of the two.
    let (sort_col, set_sort_col) = signal(SortCol::Id);
    let (sort_asc, set_sort_asc) = signal(true);

    // Filter string — updated on every keystroke in the search box.
    let (filter, set_filter) = signal(String::new());

    // The employee currently open in the edit dialog.
    // `None` means the dialog is closed; `Some(employee)` means it is open.
    let (editing, set_editing) = signal(Option::<Employee>::None);

    // ── Derived view: filtered + sorted rows ──────────────────────────────────
    //
    // This is a plain Rust closure (not a signal) that computes the visible
    // rows on demand. Leptos calls it inside the `view!` macro as a reactive
    // closure — so it re-runs whenever `filter`, `sort_col`, or `sort_asc` change.
    //
    // `.get()` on a signal inside a reactive closure creates a subscription:
    // Leptos knows this closure depends on those signals and re-runs it when
    // any of them change.
    let visible_rows = move || {
        // Lowercase the query once so we don't do it per-row per-field
        let q = filter.get().to_lowercase();

        // `rows.get()` returns a clone of the Vec<Employee>.
        // `.into_iter()` consumes it; `.filter()` keeps only matching rows.
        let mut data = rows.get()
            .into_iter()
            .filter(|e| {
                // Empty query → show all rows
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.department.to_lowercase().contains(&q)
                    || e.role.to_lowercase().contains(&q)
            })
            .collect::<Vec<_>>();

        let asc = sort_asc.get();

        // Sort by the active column.
        // `sort_by_key` is a stable sort — equal elements keep their original order.
        // `sort_by` with `.cmp()` gives alphabetical ordering for String fields.
        match sort_col.get() {
            SortCol::Id         => data.sort_by_key(|e| e.id),
            SortCol::Name       => data.sort_by(|a, b| a.name.cmp(&b.name)),
            SortCol::Department => data.sort_by(|a, b| a.department.cmp(&b.department)),
            SortCol::Salary     => data.sort_by_key(|e| e.salary),
        }

        // Reverse for descending order. `.reverse()` is O(n) in-place.
        if !asc { data.reverse(); }

        data
    };

    // ── Sort column header click handler ──────────────────────────────────────
    //
    // If clicking the already-active column → toggle asc/desc.
    // If clicking a different column → switch to it and reset to ascending.
    let on_sort = move |col: SortCol| {
        if sort_col.get() == col {
            set_sort_asc.update(|a| *a = !*a); // flip direction
        } else {
            set_sort_col.set(col);
            set_sort_asc.set(true); // new column always starts ascending
        }
    };

    // Returns the sort direction indicator for a given column.
    // Shows ▲ or ▼ only for the currently active sort column.
    let arrow = move |col: SortCol| {
        if sort_col.get() == col {
            if sort_asc.get() { " ▲" } else { " ▼" }
        } else {
            "" // not the active column — no indicator
        }
    };

    // ── Save callback ─────────────────────────────────────────────────────────
    //
    // `Callback::new` wraps a closure in Leptos's typed callback type.
    // Called by `EditDialog` when the user clicks Save.
    // Finds the row with the matching ID and replaces it in-place.
    // `.update()` takes a closure that receives a &mut reference to the value —
    // this is more efficient than `.set()` for large values because it avoids
    // cloning the entire Vec just to check if it changed.
    let on_save = Callback::new(move |updated: Employee| {
        // Optimistically update the local signal immediately so the UI
        // feels instant — no waiting for the network round-trip.
        rows.update(|list| {
            if let Some(r) = list.iter_mut().find(|r| r.id == updated.id) {
                *r = updated.clone();
            }
        });
        set_editing.set(None);

        // Then persist to the server in the background.
        // If it fails we log to console but don't roll back for simplicity.
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            if let Err(e) = save_employee(&updated).await {
                leptos::logging::log!("Save failed: {}", e);
            }
        });
    });

    // Close callback — just sets editing to None, which removes the dialog from DOM.
    let on_close = Callback::new(move |_| set_editing.set(None));

    // ── Add employee ──────────────────────────────────────────────────────────
    // `adding` controls visibility of the Add dialog.
    let (adding, set_adding) = signal(false);

    // Called when user saves a new employee from the Add dialog.
    // The server assigns the real ID; we re-fetch the full list after.
    let on_add_save = Callback::new(move |new_emp: Employee| {
        set_adding.set(false);
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            match create_employee(&new_emp).await {
                Ok(created) => {
                    // Append the newly created employee (with server-assigned ID)
                    rows.update(|list| list.push(created));
                }
                Err(e) => leptos::logging::log!("Create failed: {}", e),
            }
        });
    });

    // ── Delete employee ───────────────────────────────────────────────────────
    // `confirm_delete` holds the employee pending deletion — shows confirm dialog.
    let (confirm_delete, set_confirm_delete) = signal(Option::<Employee>::None);

    let on_delete_confirm = Callback::new(move |_| {
        if let Some(emp) = confirm_delete.get() {
            // Optimistically remove from local list
            rows.update(|list| list.retain(|r| r.id != emp.id));
            set_confirm_delete.set(None);
            // Persist deletion to server
            #[cfg(target_arch = "wasm32")]
            leptos::task::spawn_local(async move {
                if let Err(e) = delete_employee(emp.id).await {
                    leptos::logging::log!("Delete failed: {}", e);
                }
            });
        }
    });

    let on_delete_cancel = Callback::new(move |_| set_confirm_delete.set(None));

    view! {
        <section class="page">
            <h2>"Employee Data"</h2>

            // Show spinner while loading, error if fetch failed
            {move || loading.get().then(|| view! { <p class="hint">"Loading from database…"</p> })}
            {move || fetch_error.get().map(|e| view! { <p class="error">"Failed to load: " {e}</p> })}

            // ── Action toolbar: Add / Delete buttons ──────────────────────
            <div class="action-toolbar">
                // Add button — opens a blank Add Employee dialog
                <button class="btn-add"
                    on:click=move |_| set_adding.set(true)>
                    "＋ Add Employee"
                </button>
            </div>

            // ── Search + row count ─────────────────────────────────────────
            <div class="table-toolbar">
                // `on:input` fires on every keystroke (unlike `on:change` which
                // fires only on blur or Enter). This gives live filtering.
                // `event_target_value(&e)` reads the input's current string value.
                <input
                    class="table-search"
                    type="text"
                    placeholder="Filter by name, department, role…"
                    on:input=move |e| set_filter.set(event_target_value(&e))
                />
                // Reactive row count — updates as filter changes.
                // `visible_rows().len()` re-computes whenever filter/sort changes.
                <span class="table-count">
                    {move || format!("{} rows", visible_rows().len())}
                </span>
            </div>

            // ── Table ─────────────────────────────────────────────────────
            <div class="table-wrap">
                <table class="data-table">
                    <thead>
                        <tr>
                            // Sortable column headers.
                            // `on:click` fires `on_sort(col)` which updates
                            // `sort_col` and `sort_asc` signals.
                            // The header text is a reactive closure that shows
                            // the sort arrow for the active column.
                            <th class="th-sortable" on:click=move |_| on_sort(SortCol::Id)>
                                {move || format!("ID{}", arrow(SortCol::Id))}
                            </th>
                            <th class="th-sortable" on:click=move |_| on_sort(SortCol::Name)>
                                {move || format!("Name{}", arrow(SortCol::Name))}
                            </th>
                            <th class="th-sortable" on:click=move |_| on_sort(SortCol::Department)>
                                {move || format!("Department{}", arrow(SortCol::Department))}
                            </th>
                            <th>"Role"</th>
                            <th class="th-sortable" on:click=move |_| on_sort(SortCol::Salary)>
                                {move || format!("Salary{}", arrow(SortCol::Salary))}
                            </th>
                            <th>"Status"</th>
                            <th>""</th> // Edit / Delete columns — no header text
                            <th>""</th>
                        </tr>
                    </thead>
                    <tbody>
                        // Reactive row list.
                        // `move ||` makes this a reactive closure — re-runs when
                        // `visible_rows()` changes (i.e. when filter or sort changes).
                        // `.into_iter().map().collect::<Vec<_>>()` is required because
                        // Leptos needs a concrete type to render a list of views.
                        {move || visible_rows().into_iter().map(|e| {
                            // We need two separate clones of the employee because
                            // two separate closures (dblclick and button click)
                            // each take ownership via `move`. You cannot move the
                            // same value into two closures — Rust's ownership rules
                            // prevent use-after-move.
                            let e_clone_dbl = e.clone(); // owned by dblclick closure
                            let e_clone_btn = e.clone(); // owned by edit button click closure
                            let e_clone_del = e.clone(); // owned by delete button click closure

                            view! {
                                // `class=` takes a string — computed once at render time.
                                // Inactive rows are rendered with reduced opacity via CSS.
                                // `on:dblclick` — double-clicking anywhere on the row
                                // sets the `editing` signal to this employee, which
                                // causes the `EditDialog` below to appear.
                                <tr class=if e.active { "row-active" } else { "row-inactive" }
                                    style="cursor:pointer"
                                    title="Double-click to edit"
                                    on:dblclick=move |_| set_editing.set(Some(e_clone_dbl.clone()))>

                                    <td>{e.id}</td>
                                    <td>{e.name.clone()}</td>

                                    // Department shown as a pill/badge for visual grouping
                                    <td><span class="dept-badge">{e.department.clone()}</span></td>

                                    <td>{e.role.clone()}</td>

                                    // `format_salary` formats 95000 → "$95,000"
                                    <td class="td-salary">{format_salary(e.salary)}</td>

                                    <td>
                                        // Green "Active" or red "Inactive" badge
                                        <span class=if e.active { "badge badge-active" } else { "badge badge-inactive" }>
                                            {if e.active { "Active" } else { "Inactive" }}
                                        </span>
                                    </td>

                                    <td>
                                        // Edit button — alternative to double-clicking the row.
                                        <button class="btn-edit"
                                            on:click=move |_| set_editing.set(Some(e_clone_btn.clone()))>
                                            "✏ Edit"
                                        </button>
                                    </td>
                                    <td>
                                        // Delete button — opens a confirmation dialog.
                                        // Uses a third clone of the employee.
                                        <button class="btn-delete"
                                            on:click=move |_| set_confirm_delete.set(Some(e_clone_del.clone()))>
                                            "🗑"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>

            // ── Edit dialog ───────────────────────────────────────────────
            //
            // `editing.get().map(|_| ...)` returns `Some(view)` when an employee
            // is selected, and `None` when not. Leptos renders `None` as nothing,
            // so the dialog is completely absent from the DOM when closed —
            // not just hidden with CSS. This means its signals are also dropped.
            // ── Edit dialog ───────────────────────────────────────────────
            {move || editing.get().map(|_| view! {
                <EditDialog
                    employee=editing
                    on_save=on_save
                    on_close=on_close
                />
            })}

            // ── Add dialog — reuses EditDialog with a blank placeholder employee ──
            // The placeholder has id=0; the server ignores it and assigns a real ID.
            {move || adding.get().then(|| {
                // Create a temporary signal holding the blank employee
                let blank = RwSignal::new(Some(Employee {
                    id: 0,
                    name: String::new(),
                    department: "Engineering".into(),
                    role: String::new(),
                    salary: 0,
                    active: true,
                }));
                view! {
                    <EditDialog
                        employee=blank.read_only()
                        on_save=on_add_save
                        on_close=Callback::new(move |_| set_adding.set(false))
                    />
                }
            })}

            // ── Confirm delete dialog ──────────────────────────────────────
            {move || confirm_delete.get().map(|emp| view! {
                <div class="dialog-backdrop" on:click=move |_| on_delete_cancel.run(())>
                    <div class="dialog dialog-confirm" on:click=move |e| e.stop_propagation()>
                        <div class="dialog-header">
                            <h3>"Delete Employee"</h3>
                            <button class="dialog-close" on:click=move |_| on_delete_cancel.run(())>"✕"</button>
                        </div>
                        <div class="dialog-body">
                            <p>"Are you sure you want to delete "<strong>{emp.name.clone()}</strong>"?"</p>
                            <p class="hint">"This action cannot be undone."</p>
                        </div>
                        <div class="dialog-footer">
                            <button class="btn-cancel" on:click=move |_| on_delete_cancel.run(())>"Cancel"</button>
                            <button class="btn-danger" on:click=move |_| on_delete_confirm.run(())>"Delete"</button>
                        </div>
                    </div>
                </div>
            })}
        </section>
    }
}

/// Formats a salary integer as a USD string with thousands separators.
///
/// Example: `95000` → `"$95,000"`
///
/// Algorithm: reverse the digits, insert a comma every 3 characters,
/// then reverse back. This avoids needing locale-aware formatting libraries.
fn format_salary(s: i32) -> String {
    let s = s.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); } // insert thousands separator
        out.push(c);
    }
    // Reverse again to restore correct order, then prepend "$"
    format!("${}", out.chars().rev().collect::<String>())
}

// ─────────────────────────────────────────────────────────────────────────────
// Server counter page
// ─────────────────────────────────────────────────────────────────────────────

/// Demonstrates server-side state shared across all browser sessions.
///
/// The counter value lives in `Arc<Mutex<i32>>` on the Axum server.
/// The page fetches the current value on load and POSTs increment/decrement
/// actions. Because the state is on the server, all browser tabs see the
/// same counter value.
#[component]
fn CounterPage() -> impl IntoView {

    // `state` holds the last-fetched CounterState from the server.
    // Starts as `None` so we can show "Loading..." until the first fetch completes.
    let state = RwSignal::new(Option::<CounterState>::None);

    // `error` holds an error message string if any HTTP request fails.
    let error = RwSignal::new(Option::<String>::None);

    // Fetch the initial counter value from the server when the component mounts.
    // `Effect::new` runs after render and re-runs when its signal dependencies change.
    // Here it has no signal dependencies so it runs exactly once on mount.
    Effect::new(move |_| {
        // `spawn_local` schedules an async task on the WASM single-threaded executor.
        // WASM cannot use `std::thread::spawn` — only `spawn_local`.
        // This block is compiled out on the server (which uses tokio threads instead).
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                match fetch_counter().await {
                    Ok(s)  => state.set(Some(s)), // success — show the counter
                    Err(e) => error.set(Some(e)), // failure — show the error message
                }
            });
        }
    });

    // Closure that POSTs to /api/increment and updates the state signal.
    // `move |_|` captures `state` and `error` by move — the browser event
    // object is ignored (named `_`).
    let increment = move |_| {
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            match post_action("/api/increment").await {
                Ok(s)  => state.set(Some(s)),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Same pattern for decrement.
    let decrement = move |_| {
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            match post_action("/api/decrement").await {
                Ok(s)  => state.set(Some(s)),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <section class="page">
            <h2>"Server Counter"</h2>
            <p class="hint">"State lives on the server — shared across all browser tabs."</p>

            // Show error banner if any request failed.
            // `error.get().map(...)` returns None (renders nothing) when no error.
            {move || error.get().map(|e| view! { <p class="error">"Error: " {e}</p> })}

            // Show the counter widget once state has been fetched.
            // While state is None (still loading), this renders nothing.
            {move || state.get().map(|s| view! {
                <div class="counter-box">
                    <p class="counter-label">{s.label}</p>
                    <p class="counter-value">{s.value}</p>
                    <div class="counter-buttons">
                        <button on:click=decrement class="btn btn-dec">"−"</button>
                        <button on:click=increment class="btn btn-inc">"+"</button>
                    </div>
                </div>
            })}

            // Show "Loading..." while state is None.
            // `.then()` returns Some(view) if the condition is true, None otherwise.
            {move || state.get().is_none().then(|| view! { <p>"Loading..."</p> })}
        </section>
    }
}

/// Fetches all employees from GET /api/employees.
/// Returns the full list from Postgres.
#[cfg(target_arch = "wasm32")]
async fn fetch_employees() -> Result<Vec<Employee>, String> {
    let resp = gloo_net::http::Request::get("/api/employees")
        .send().await
        .map_err(|e| e.to_string())?;
    let body: ApiResponse<Vec<Employee>> = resp.json().await
        .map_err(|e| e.to_string())?;
    body.data.ok_or_else(|| body.error.unwrap_or_default())
}

/// POSTs an updated employee to PUT /api/employees/:id.
/// The server writes the change to Postgres.
#[cfg(target_arch = "wasm32")]
async fn save_employee(emp: &Employee) -> Result<(), String> {
    let url = format!("/api/employees/{}", emp.id);
    let resp = gloo_net::http::Request::post(&url)
        .json(emp)
        .map_err(|e| e.to_string())?
        .send().await
        .map_err(|e| e.to_string())?;
    let body: ApiResponse<Employee> = resp.json().await
        .map_err(|e| e.to_string())?;
    if body.ok { Ok(()) } else { Err(body.error.unwrap_or_default()) }
}

/// POSTs a new employee to POST /api/employees — server inserts and returns with real ID.
#[cfg(target_arch = "wasm32")]
async fn create_employee(emp: &Employee) -> Result<Employee, String> {
    let resp = gloo_net::http::Request::post("/api/employees")
        .json(emp)
        .map_err(|e| e.to_string())?
        .send().await
        .map_err(|e| e.to_string())?;
    let body: ApiResponse<Employee> = resp.json().await
        .map_err(|e| e.to_string())?;
    body.data.ok_or_else(|| body.error.unwrap_or_default())
}

/// DELETEs an employee via DELETE /api/employees/:id.
#[cfg(target_arch = "wasm32")]
async fn delete_employee(id: i32) -> Result<(), String> {
    let url = format!("/api/employees/{}", id);
    let resp = gloo_net::http::Request::delete(&url)
        .send().await
        .map_err(|e| e.to_string())?;
    let body: ApiResponse<()> = resp.json().await
        .map_err(|e| e.to_string())?;
    if body.ok { Ok(()) } else { Err(body.error.unwrap_or_default()) }
}

/// Fetches the current counter state from GET /api/counter.
///
/// Returns `Ok(CounterState)` on success or `Err(String)` with an error message.
/// `gloo_net` is a WASM-friendly HTTP client — it uses the browser's `fetch` API.
/// Only compiled for WASM — the server never calls its own REST endpoints.
#[cfg(target_arch = "wasm32")]
async fn fetch_counter() -> Result<CounterState, String> {
    // `.send()` performs the HTTP request asynchronously.
    // `.map_err(|e| e.to_string())` converts the error type from gloo_net's
    // error enum to a plain String for uniform error handling.
    // `?` propagates the error early if the request failed.
    let resp = gloo_net::http::Request::get("/api/counter")
        .send().await
        .map_err(|e| e.to_string())?;

    // `.json()` deserializes the response body using serde.
    // The type annotation `ApiResponse<CounterState>` tells serde what shape to expect.
    let body: ApiResponse<CounterState> = resp.json().await
        .map_err(|e| e.to_string())?;

    // `ApiResponse` wraps the actual data in an `Option` — unwrap it or
    // return the server's error message if `data` is None.
    body.data.ok_or_else(|| body.error.unwrap_or_default())
}

/// POSTs to a server action endpoint (increment or decrement) and returns
/// the updated counter state.
///
/// `url` is either "/api/increment" or "/api/decrement".
/// Only compiled for WASM — same reason as `fetch_counter`.
#[cfg(target_arch = "wasm32")]
async fn post_action(url: &str) -> Result<CounterState, String> {
    let resp = gloo_net::http::Request::post(url)
        .send().await
        .map_err(|e| e.to_string())?;
    let body: ApiResponse<CounterState> = resp.json().await
        .map_err(|e| e.to_string())?;
    body.data.ok_or_else(|| body.error.unwrap_or_default())
}

// ─────────────────────────────────────────────────────────────────────────────
// Local counter page
// ─────────────────────────────────────────────────────────────────────────────

/// Pure client-side counter — state lives in a WASM signal, not on the server.
///
/// Demonstrates the simplest possible reactive pattern:
///   1. Create a signal with an initial value.
///   2. Read it in a reactive closure `{move || count.get()}`.
///   3. Write it in a button handler `set_count.update(|n| *n += 1)`.
///
/// The value resets to 0 on page refresh because it is never persisted.
#[component]
fn LocalCounterPage() -> impl IntoView {
    // `signal(0i32)` creates a signal holding a 32-bit integer.
    // `count`     — ReadSignal<i32>: read-only handle
    // `set_count` — WriteSignal<i32>: write-only handle
    let (count, set_count) = signal(0i32);

    view! {
        <section class="page">
            <h2>"Local Counter"</h2>
            <p class="hint">"Pure client-side — state lives in the browser, resets on refresh."</p>

            <div class="counter-box">
                <p class="counter-label">"Count"</p>

                // `{move || count.get()}` is the reactive read.
                // Writing just `{count.get()}` would evaluate once at render
                // time and never update. The `move ||` closure creates a
                // reactive subscription — Leptos re-runs it when `count` changes.
                <p class="counter-value">{move || count.get()}</p>

                <div class="counter-buttons">
                    // `set_count.update(|n| *n += 1)` dereferences the &mut i32
                    // and increments it in-place. This notifies all subscribers
                    // (the counter-value closure above) to re-run.
                    <button class="btn btn-inc"
                        on:click=move |_| set_count.update(|n| *n += 1)>
                        "+"
                    </button>
                </div>
            </div>
        </section>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Called by the JavaScript glue code generated by `wasm-bindgen` after the
/// WASM binary has been downloaded and instantiated by the browser.
///
/// `#[wasm_bindgen]` exports this function so JS can call `mod.hydrate()`.
///
/// `hydrate_body(App)` walks the existing SSR HTML in `<body>` and attaches
/// Leptos's reactive graph to it — wiring up signals and event listeners
/// without re-rendering. The user sees the server-rendered HTML immediately,
/// and interactivity kicks in once WASM loads.
///
/// Without this function the WASM loads but does nothing — the page would
/// look correct but buttons would be unresponsive.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use leptos::mount::hydrate_body;
    hydrate_body(App);
}
