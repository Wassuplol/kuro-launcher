pub mod discovery;
pub mod error;
pub mod flags;
pub mod http;
pub mod identity;
pub mod launch;
pub mod library;
pub mod settings;
pub mod social;
pub mod supply;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub use error::{FailureCode, LaunchError};
use settings::Settings;
use supply::install::{Method, Progress, VerifyReport, EVENT};
use supply::prune::PruneResult;
use supply::{Kind, Snapshot, SupplyState};

/// The smallest the main window may be after the opening has finished. Below
/// this the rail and the page content start colliding.
const FLOOR: (f64, f64) = (920.0, 620.0);

pub struct AppState {
    pub data_root: PathBuf,
    pub settings: Mutex<Settings>,
    pub settings_error: Option<LaunchError>,
    /// Held for the life of the app while multi-instance is on. Dropping it
    /// hands the lock back to Roblox, which is the whole mechanism.
    pub singleton: Mutex<Option<launch::spawn::Singleton>>,
    /// A flag file that was put back after a crash, reported once so the screen
    /// can say what happened instead of leaving it invisible.
    pub recovered: Mutex<Option<String>>,
    pub supply: SupplyState,
    /// The per-class request allowance and the answers kept between paints.
    pub budget: std::sync::Arc<discovery::Budget>,
    pub cache: std::sync::Arc<discovery::Cache>,
    /// Set once the page has taken control of the window. Until then the window
    /// stays hidden, so nothing full-size is ever painted before the opening.
    pub opened: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub data_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_error: Option<LaunchError>,
}

fn poisoned() -> LaunchError {
    LaunchError::new(
        FailureCode::Internal,
        "The settings lock was poisoned by an earlier fault.",
    )
}

fn worker_failed(detail: String) -> LaunchError {
    LaunchError::new(FailureCode::Internal, detail)
}

#[tauri::command]
fn app_info(app: tauri::AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        identifier: app.config().identifier.clone(),
        data_root: state.data_root.to_string_lossy().to_string(),
        settings_error: state.settings_error.clone(),
    }
}

#[tauri::command]
fn settings_load(state: State<'_, AppState>) -> Result<Settings, LaunchError> {
    let guard = state.settings.lock().map_err(|_| poisoned())?;
    Ok(guard.clone())
}

#[tauri::command]
fn settings_save(state: State<'_, AppState>, next: Settings) -> Result<Settings, LaunchError> {
    let next = settings::normalise(next);
    settings::save(&state.data_root, &next)?;
    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    *guard = next.clone();
    Ok(next)
}

/// Recovery for a settings file that cannot be read. Writes the defaults back
/// and returns them, so a broken file never leaves the app without a way out.
#[tauri::command]
fn settings_reset(state: State<'_, AppState>) -> Result<Settings, LaunchError> {
    let fresh = Settings::default();
    settings::save(&state.data_root, &fresh)?;
    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    *guard = fresh.clone();
    Ok(fresh)
}

/// Opens the data root in Explorer. Spawned directly so the frontend never
/// receives a filesystem path it could pass somewhere else.
#[tauri::command]
fn open_data_root(state: State<'_, AppState>) -> Result<(), LaunchError> {
    std::process::Command::new("explorer")
        .arg(&state.data_root)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Explorer could not open the data folder: {e}"),
            )
        })
}

/// Shrinks the window to the opening scene and shows it.
///
/// The page asks for this, rather than the setup doing it, because the page is
/// the only part of the app that knows how big the window actually is. The
/// window-state plugin restores the last size after the window is created, so a
/// read during setup gets the size from the config file and quietly forgets the
/// size this person actually uses. The page reads `innerWidth` and `innerHeight`,
/// which are already the restored values, remembers them as the destination, and
/// hands the window back here.
#[tauri::command]
fn window_open(
    window: tauri::Window,
    state: State<'_, AppState>,
    width: f64,
    height: f64,
) -> Result<(), LaunchError> {
    window
        .set_min_size(Some(tauri::LogicalSize::new(width, height)))
        .map_err(|e| LaunchError::new(FailureCode::Internal, format!("No minimum size: {e}")))?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| LaunchError::new(FailureCode::Internal, format!("No window size: {e}")))?;
    let _ = window.center();
    window
        .show()
        .map_err(|e| LaunchError::new(FailureCode::Internal, format!("No window: {e}")))?;

    state.opened.store(true, Ordering::SeqCst);
    Ok(())
}

/// Resizes the window from where it is to `width` by `height`, growing around a
/// fixed centre so the opening scene stays still while the window opens up
/// around it.
///
/// This runs in Rust rather than being stepped from the page because it is a
/// resize per frame: around ninety of them across the transition, and not one of
/// them should cost a round trip through the IPC bridge.
#[tauri::command]
async fn window_grow(
    window: tauri::Window,
    width: f64,
    height: f64,
    duration_ms: u64,
) -> Result<(), LaunchError> {
    tauri::async_runtime::spawn_blocking(move || {
        let min = (width.min(FLOOR.0), height.min(FLOOR.1));
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(min.0, min.1)));

        let scale = window.scale_factor().unwrap_or(1.0);
        let target = (
            (width * scale).round().max(1.0),
            (height * scale).round().max(1.0),
        );

        let start = window
            .inner_size()
            .map_err(|e| LaunchError::new(FailureCode::Internal, format!("No window size: {e}")))?;
        let origin = window.outer_position().map_err(|e| {
            LaunchError::new(FailureCode::Internal, format!("No window position: {e}"))
        })?;

        // The centre of the window on screen. Everything is interpolated around
        // it, so a symmetric growth leaves the middle of the scene untouched.
        let centre = (
            origin.x as f64 + start.width as f64 / 2.0,
            origin.y as f64 + start.height as f64 / 2.0,
        );

        // Driven by the clock rather than by a step count. Each frame costs the
        // two window calls plus the sleep, and that overhead is not free: a
        // fixed number of steps ran about forty percent long, which left the
        // content finishing its fade while the window was still opening.
        let began = std::time::Instant::now();

        loop {
            let elapsed = began.elapsed().as_millis() as f64;
            let progress = if duration_ms == 0 {
                1.0
            } else {
                (elapsed / duration_ms as f64).min(1.0)
            };
            let eased = bezier(progress);

            let width = lerp(start.width as f64, target.0, eased).round() as u32;
            let height = lerp(start.height as f64, target.1, eased).round() as u32;

            let _ = window.set_size(tauri::PhysicalSize::new(width.max(1), height.max(1)));
            let _ = window.set_position(tauri::PhysicalPosition::new(
                (centre.0 - width as f64 / 2.0).round() as i32,
                (centre.1 - height as f64 / 2.0).round() as i32,
            ));

            if progress >= 1.0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }

        Ok(())
    })
    .await
    .map_err(|e| worker_failed(format!("The window could not be resized: {e}")))?
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

/// `cubic-bezier(0.2, 0.8, 0.2, 1)`, the app's one easing, solved by bisection.
///
/// Solving rather than approximating keeps the resize in step with every CSS
/// transition around it: the window and the content it reveals decelerate at the
/// same rate, which is the whole point of the growth reading as one motion.
fn bezier(t: f64) -> f64 {
    const X1: f64 = 0.2;
    const Y1: f64 = 0.8;
    const X2: f64 = 0.2;
    const Y2: f64 = 1.0;

    let mut low = 0.0f64;
    let mut high = 1.0f64;
    let mut u = t;

    for _ in 0..24 {
        let delta = curve(u, X1, X2) - t;
        if delta.abs() < 1e-5 {
            break;
        }
        if delta < 0.0 {
            low = u;
        } else {
            high = u;
        }
        u = (low + high) / 2.0;
    }

    curve(u, Y1, Y2)
}

/// One axis of a cubic bezier with endpoints at 0 and 1.
fn curve(u: f64, p1: f64, p2: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u
}

/// Every account saved on this machine, without a cookie in sight.
#[tauri::command]
fn accounts_list(state: State<'_, AppState>) -> Result<Vec<identity::AccountView>, LaunchError> {
    Ok(identity::views(&identity::load(&state.data_root)?))
}

/// Adds an account from a pasted cookie, verifying it against Roblox before
/// anything is stored. The cookie reaches Rust and stops there: it is sealed
/// into the store and never returned to the page.
#[tauri::command]
async fn accounts_import(
    state: State<'_, AppState>,
    cookie: String,
) -> Result<Vec<identity::AccountView>, LaunchError> {
    let root = state.data_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = identity::import(&root, &cookie)?;
        Ok(identity::views(&store))
    })
    .await
    .map_err(|e| worker_failed(format!("The account could not be added: {e}")))?
}

/// Re-checks every saved session against Roblox and records the answer.
#[tauri::command]
async fn accounts_check(
    state: State<'_, AppState>,
) -> Result<Vec<identity::AccountView>, LaunchError> {
    let root = state.data_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = identity::check_all(&root)?;
        Ok(identity::views(&store))
    })
    .await
    .map_err(|e| worker_failed(format!("The check did not finish: {e}")))?
}

#[tauri::command]
fn accounts_remove(
    state: State<'_, AppState>,
    user_id: u64,
) -> Result<Vec<identity::AccountView>, LaunchError> {
    let store = identity::remove(&state.data_root, user_id)?;
    Ok(identity::views(&store))
}

#[tauri::command]
fn accounts_default(
    state: State<'_, AppState>,
    user_id: u64,
) -> Result<Vec<identity::AccountView>, LaunchError> {
    let store = identity::set_default(&state.data_root, user_id)?;
    Ok(identity::views(&store))
}

/// Starts a Quick Login attempt: Roblox's own cross-device sign-in, so nothing
/// is typed into this app and nothing is pasted out of a browser.
#[tauri::command]
async fn quicklogin_start() -> Result<identity::quicklogin::Attempt, LaunchError> {
    tauri::async_runtime::spawn_blocking(identity::quicklogin::create)
        .await
        .map_err(|e| worker_failed(format!("The sign-in code was not created: {e}")))?
}

/// One poll of an attempt. The page owns the timer, so an attempt cannot be
/// left running in Rust after the panel closes.
#[tauri::command]
async fn quicklogin_poll(
    code: String,
    private_key: String,
) -> Result<identity::quicklogin::Poll, LaunchError> {
    tauri::async_runtime::spawn_blocking(move || identity::quicklogin::poll(&code, &private_key))
        .await
        .map_err(|e| worker_failed(format!("The sign-in code was not checked: {e}")))?
}

/// Frees a pending approval the person backed out of.
#[tauri::command]
async fn quicklogin_cancel(code: String, private_key: String) -> Result<(), LaunchError> {
    tauri::async_runtime::spawn_blocking(move || identity::quicklogin::cancel(&code, &private_key))
        .await
        .map_err(|e| worker_failed(format!("The sign-in code was not cancelled: {e}")))?
}

/// Exchanges an approved code for a session and stores it sealed. The cookie is
/// created, verified and sealed inside this one call; it never reaches the page.
#[tauri::command]
async fn quicklogin_finish(
    state: State<'_, AppState>,
    code: String,
    private_key: String,
) -> Result<Vec<identity::AccountView>, LaunchError> {
    let root = state.data_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cookie = identity::quicklogin::exchange(&code, &private_key)?;
        let store = identity::store_session(&root, &cookie)?;
        Ok(identity::views(&store))
    })
    .await
    .map_err(|e| worker_failed(format!("The approval was not stored: {e}")))?
}

/// Opens a Roblox page in the default browser.
///
/// The URL comes from the renderer, so it is checked against Roblox's own hosts
/// first: a command that opens anything would be a way to launch a local file or
/// another program from the page. Discovery and game pages need the same door
/// later, which is why this is a general helper and not a Quick Login one.
#[tauri::command]
fn open_site(url: String) -> Result<(), LaunchError> {
    const HOSTS: [&str; 4] = [
        "https://www.roblox.com/",
        "https://web.roblox.com/",
        "https://apis.roblox.com/",
        "https://auth.roblox.com/",
    ];

    if !HOSTS.iter().any(|host| url.starts_with(host)) {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "That is not a Roblox address, so it was not opened.",
        ));
    }

    std::process::Command::new("explorer")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Your browser could not be opened: {e}"),
            )
        })
}

/// Writes the flags that apply to one launch, and says what happened either way.
fn apply_flags_for(
    root: &std::path::Path,
    installed: &[supply::InstalledClient],
    pinned: &Option<String>,
    account: u64,
    place: Option<u64>,
) -> (bool, Option<String>) {
    let store = match flags::store::load(root) {
        Ok(store) => store,
        Err(error) => return (false, Some(error.message)),
    };

    let rows = flags::store::resolve(&store, Some(account), place);
    if rows.is_empty() {
        return (
            false,
            Some("No flags apply to this account and place.".to_string()),
        );
    }

    let target = match flags::target(installed, pinned) {
        Ok(target) => target,
        Err(error) => return (false, Some(error.message)),
    };

    // The count Roblox will actually apply on the next boot. Saying "5 flags
    // written" over a file the client drops four of is the sentence the
    // allowlist turned into a lie.
    let honoured = flags::allow::count(rows.iter().map(|row| row.key.as_str()));

    match flags::apply::apply(root, std::path::Path::new(&target.path), &rows) {
        Ok(lease) if honoured == lease.keys => (
            true,
            Some(format!("{} flags written to {}", lease.keys, lease.path)),
        ),
        Ok(lease) => (
            true,
            Some(format!(
                "{} flags written to {}; the client honours {} of them and ignores the rest",
                lease.keys, lease.path, honoured
            )),
        ),
        Err(error) => (false, Some(error.message)),
    }
}

/// Reads a destination without starting anything: what mode it is, which place
/// it names, and which client would run it.
#[tauri::command]
async fn launch_read(
    state: State<'_, AppState>,
    target: String,
) -> Result<launch::Reading, LaunchError> {
    let pinned = {
        let settings = state.settings.lock().map_err(|_| poisoned())?;
        settings.supply.pinned.clone()
    };

    tauri::async_runtime::spawn_blocking(move || {
        let installed = supply::discover_all(&pinned, None)?;
        launch::reading(&target, &installed, &pinned)
    })
    .await
    .map_err(|e| worker_failed(format!("That destination could not be read: {e}")))?
}

/// One launch: parse, preflight, mint a ticket, build the string, spawn.
///
/// The ticket is minted here and used once. It is never returned to the page,
/// never stored, and never reused, because it is single use by design.
#[tauri::command]
async fn launch_start(
    state: State<'_, AppState>,
    target: String,
    user_id: Option<u64>,
) -> Result<launch::Launched, LaunchError> {
    let root = state.data_root.clone();
    let (pinned, multi) = {
        let settings = state.settings.lock().map_err(|_| poisoned())?;
        (
            settings.supply.pinned.clone(),
            settings.launch.multi_instance,
        )
    };

    tauri::async_runtime::spawn_blocking(move || {
        let installed = supply::discover_all(&pinned, None)?;
        let prepared = launch::prepare(&target, &installed, &pinned)?;

        let store = identity::load(&root)?;
        let chosen = user_id
            .or(store.default)
            .or_else(|| store.accounts.first().map(|account| account.user_id));

        let Some(chosen) = chosen else {
            return Err(LaunchError::new(
                FailureCode::SessionExpired,
                "There is no account on this machine to launch as.",
            )
            .with_action("Add an account"));
        };

        let account = store
            .accounts
            .iter()
            .find(|account| account.user_id == chosen)
            .ok_or_else(|| {
                LaunchError::new(
                    FailureCode::NotFound,
                    "That account is not saved on this machine.",
                )
                .with_action("Add the account again")
            })?;
        let display = account.display.clone();

        // Said before a ticket is minted, so a machine that cannot run a second
        // client is told that instead of being handed a launch that focuses the
        // window it already has.
        if !multi && launch::spawn::player_running() {
            return Err(LaunchError::new(
                FailureCode::AlreadyRunning,
                "Roblox is already running. The client focuses the window it has instead of \
                 starting a second one.",
            )
            .with_action("Allow more than one game"));
        }

        let cookie = identity::cookie_for(&root, chosen)?;

        let key = format!("{chosen}|{target}");
        let reused = launch::begin(&key)?;

        let mode = prepared.mode;
        let detail = match mode {
            launch::Mode::Normal => format!(
                "Opened place {} in any server as {display}.",
                prepared.place.unwrap_or_default()
            ),
            launch::Mode::Server => format!(
                "Opened one server of place {} as {display}.",
                prepared.place.unwrap_or_default()
            ),
            launch::Mode::Private => format!(
                "Opened the private server for place {} as {display}.",
                prepared.place.unwrap_or_default()
            ),
            launch::Mode::Follow => format!("Following a friend as {display}."),
        };

        if reused {
            return Ok(launch::Launched {
                mode,
                mode_label: prepared.mode_label,
                place: prepared.place,
                server: prepared.server,
                account: display,
                client: prepared.client,
                version: prepared.version,
                reused: true,
                detail: "That launch is already starting, so no second client was opened."
                    .to_string(),
                flags: None,
            });
        }

        // Flags go in before the client starts, because the client reads the
        // file once at boot. A failure here is reported and the launch goes
        // ahead: a game with vanilla flags beats no game.
        let (leashed, flag_note) =
            apply_flags_for(&root, &installed, &pinned, chosen, prepared.place);

        let outcome = (|| {
            let ticket = launch::ticket::mint(&cookie)?;
            let uri = launch::launch_uri(&ticket, &prepared.launcher_url, launch::now_ms());
            launch::spawn::spawn(std::path::Path::new(&prepared.path), &uri)
        })();

        match outcome {
            Ok(mut child) => {
                launch::settle(&key);
                // Recorded so "continue as" has an answer next time. A failure
                // here must not turn a running client into an error.
                let _ = identity::mark_used(&root, chosen);

                if leashed {
                    let data = root.clone();
                    std::thread::spawn(move || {
                        let _ = child.wait();
                        // A second client may still be running, and it read the
                        // same file at its own boot. The lease ends when the
                        // last client does.
                        for _ in 0..900 {
                            if !launch::spawn::player_running() {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        let _ = flags::apply::revert(&data);
                    });
                }

                Ok(launch::Launched {
                    mode,
                    mode_label: prepared.mode_label,
                    place: prepared.place,
                    server: prepared.server,
                    account: display,
                    client: prepared.client,
                    version: prepared.version,
                    reused: false,
                    detail,
                    flags: flag_note,
                })
            }
            Err(error) => {
                launch::release();
                // A failure after the flags were written must not leave them
                // behind for a session that never started.
                if leashed {
                    let _ = flags::apply::revert(&root);
                }
                Err(error)
            }
        }
    })
    .await
    .map_err(|e| worker_failed(format!("The launch did not finish: {e}")))?
}

/// Turns the multi-instance lock on or off and remembers the answer.
///
/// Off by default, because holding this mutex changes how every Roblox program
/// on the machine behaves, and that is the person's decision to make.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiInstance {
    pub settings: Settings,
    /// False when another program already held the mutex, which means the
    /// switch applies from its next start rather than from now.
    pub created: bool,
}

#[tauri::command]
fn launch_multi_instance(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<MultiInstance, LaunchError> {
    let mut slot = state.singleton.lock().map_err(|_| poisoned())?;
    let mut created = true;
    if enabled {
        if slot.is_none() {
            let held = launch::spawn::hold_singleton()?;
            created = held.created;
            *slot = Some(held);
        }
    } else {
        // Dropping the handle releases the lock.
        *slot = None;
    }
    drop(slot);

    let next = {
        let mut settings = state.settings.lock().map_err(|_| poisoned())?;
        settings.launch.multi_instance = enabled;
        settings::normalise(settings.clone())
    };
    settings::save(&state.data_root, &next)?;

    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    *guard = next.clone();
    Ok(MultiInstance {
        settings: next,
        created,
    })
}

/// What the launch panel needs to know before anything is pressed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchState {
    /// A game client is running right now.
    pub player_running: bool,
    pub multi_instance: bool,
}

#[tauri::command]
fn launch_state(state: State<'_, AppState>) -> Result<LaunchState, LaunchError> {
    let multi_instance = state
        .settings
        .lock()
        .map_err(|_| poisoned())?
        .launch
        .multi_instance;

    Ok(LaunchState {
        player_running: launch::spawn::player_running(),
        multi_instance,
    })
}

// ---- stage 05, flags ----

/// Everything the flags drawer needs in one answer: the catalogue's age, the
/// profiles, where a write would land, and any lease still in force.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagsView {
    pub catalog: Option<flags::catalog::Catalog>,
    pub store: flags::Store,
    pub target: Option<flags::Target>,
    pub lease: Option<flags::Lease>,
    pub exports: Vec<flags::ExportFile>,
    /// A flag file that was put back on this start because the last run did not
    /// finish cleanly.
    pub recovered: Option<String>,
    /// The keys the client takes from a local file, so the page can say which
    /// of the person's flags will actually do anything.
    pub allowlist: &'static [&'static str],
    pub allowlist_source: &'static str,
    pub player_running: bool,
}

fn installed_clients(
    state: &State<'_, AppState>,
) -> Result<(Vec<supply::InstalledClient>, Option<String>), LaunchError> {
    let pinned = state
        .settings
        .lock()
        .map_err(|_| poisoned())?
        .supply
        .pinned
        .clone();
    let installed = supply::discover_all(&pinned, None)?;
    Ok((installed, pinned))
}

#[tauri::command]
async fn flags_view(state: State<'_, AppState>) -> Result<FlagsView, LaunchError> {
    let root = state.data_root.clone();
    let (installed, pinned) = installed_clients(&state)?;
    let recovered = state.recovered.lock().map_err(|_| poisoned())?.take();

    Ok(FlagsView {
        catalog: flags::catalog::load(&root),
        store: flags::store::load(&root)?,
        target: flags::target(&installed, &pinned).ok(),
        lease: flags::apply::lease(&root),
        exports: flags::exports(&root),
        recovered,
        allowlist: &flags::allow::KEYS,
        allowlist_source: flags::allow::SOURCE,
        player_running: launch::spawn::player_running(),
    })
}

/// Fetches Roblox's own flag list. The catalogue is what the client itself
/// reads, so a key's default here is the client's default.
#[tauri::command]
async fn flags_refresh(state: State<'_, AppState>) -> Result<flags::catalog::Catalog, LaunchError> {
    let root = state.data_root.clone();
    tauri::async_runtime::spawn_blocking(move || flags::catalog::refresh(&root))
        .await
        .map_err(|e| worker_failed(format!("The flag list was not refreshed: {e}")))?
}

#[tauri::command]
fn flags_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<flags::catalog::Hit>, LaunchError> {
    let catalog = flags::catalog::load(&state.data_root).unwrap_or(flags::catalog::Catalog {
        fetched: 0,
        application: String::new(),
        values: Default::default(),
    });
    Ok(flags::catalog::search(
        &catalog,
        &query,
        limit.unwrap_or(40),
    ))
}

/// Replaces one profile's whole flag set. The page sends the set it is showing,
/// so what is saved is exactly what the person can see.
#[tauri::command]
fn flags_save_profile(
    state: State<'_, AppState>,
    name: String,
    values: std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<flags::Store, LaunchError> {
    for (key, value) in &values {
        flags::store::check_value(key, value)?;
    }

    let mut store = flags::store::load(&state.data_root)?;
    flags::store::upsert_profile(&mut store, &name, values)?;
    flags::store::save(&state.data_root, &store)?;
    Ok(store)
}

#[tauri::command]
fn flags_delete_profile(
    state: State<'_, AppState>,
    name: String,
) -> Result<flags::Store, LaunchError> {
    let mut store = flags::store::load(&state.data_root)?;
    flags::store::drop_profile(&mut store, &name);
    flags::store::save(&state.data_root, &store)?;
    Ok(store)
}

/// Sets one slot of the scope chain. `profile` of `None` clears that slot.
#[tauri::command]
fn flags_set_scope(
    state: State<'_, AppState>,
    slot: String,
    id: Option<u64>,
    profile: Option<String>,
) -> Result<flags::Store, LaunchError> {
    let mut store = flags::store::load(&state.data_root)?;

    if let Some(name) = &profile {
        if flags::store::profile_named(&store, name).is_none() {
            return Err(LaunchError::new(
                FailureCode::NotFound,
                format!("There is no profile called {name}."),
            ));
        }
    }

    match slot.as_str() {
        "global" => store.scopes.global = profile,
        "account" => match (id, profile) {
            (Some(id), Some(name)) => {
                store.scopes.accounts.insert(id.to_string(), name);
            }
            (Some(id), None) => {
                store.scopes.accounts.remove(&id.to_string());
            }
            _ => {
                return Err(LaunchError::new(
                    FailureCode::NotFound,
                    "An account scope needs an account.",
                ))
            }
        },
        "game" => match (id, profile) {
            (Some(id), Some(name)) => {
                store.scopes.games.insert(id.to_string(), name);
            }
            (Some(id), None) => {
                store.scopes.games.remove(&id.to_string());
            }
            _ => {
                return Err(LaunchError::new(
                    FailureCode::NotFound,
                    "A game scope needs a place id.",
                ))
            }
        },
        other => {
            return Err(LaunchError::new(
                FailureCode::NotFound,
                format!("There is no {other} scope."),
            ))
        }
    }

    flags::store::save(&state.data_root, &store)?;
    Ok(store)
}

/// The resolved chain for one account and one place, with provenance per row.
#[tauri::command]
fn flags_resolve(
    state: State<'_, AppState>,
    account: Option<u64>,
    place: Option<u64>,
) -> Result<Vec<flags::Row>, LaunchError> {
    let store = flags::store::load(&state.data_root)?;
    Ok(flags::store::resolve(&store, account, place))
}

/// Writes the resolved flags now, outside a launch. A live client makes this a
/// refusal, because the file is read at boot and would not reach that session.
#[tauri::command]
fn flags_apply(
    state: State<'_, AppState>,
    account: Option<u64>,
    place: Option<u64>,
) -> Result<flags::Lease, LaunchError> {
    if launch::spawn::player_running() {
        return Err(LaunchError::new(
            FailureCode::AlreadyRunning,
            "A client is running, and it read its flags when it started. Close it first.",
        )
        .with_action("Close Roblox"));
    }

    let store = flags::store::load(&state.data_root)?;
    let rows = flags::store::resolve(&store, account, place);
    if rows.is_empty() {
        return Err(LaunchError::new(
            FailureCode::NotFound,
            "No profile sets any flag for that account and place, so there is nothing to write.",
        )
        .with_action("Add a flag"));
    }

    let (installed, pinned) = installed_clients(&state)?;
    let target = flags::target(&installed, &pinned)?;
    flags::apply::apply(&state.data_root, std::path::Path::new(&target.path), &rows)
}

#[tauri::command]
fn flags_revert(state: State<'_, AppState>) -> Result<Option<String>, LaunchError> {
    flags::apply::revert(&state.data_root)
}

/// The home feed, hydrated into cards the grid can draw.
///
/// Needs the session, because Roblox builds this feed for a signed-in account.
#[tauri::command]
async fn discovery_home(state: State<'_, AppState>) -> Result<discovery::Feed, LaunchError> {
    let root = state.data_root.clone();
    let chosen = identity::load(&root).ok().and_then(|store| {
        store
            .default
            .or_else(|| store.accounts.first().map(|a| a.user_id))
    });

    let cookie = match chosen {
        Some(user) => identity::cookie_for(&root, user).ok(),
        None => None,
    };

    let budget = state.budget.clone();
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        discovery::home(&budget, &cache, cookie.as_deref())
    })
    .await
    .map_err(|e| worker_failed(format!("The home feed was not read: {e}")))?
}

/// Games or people for one query. One call per search, because the endpoint
/// allows four a minute and the cache answers repeats for three.
#[tauri::command]
async fn discovery_search(
    state: State<'_, AppState>,
    term: String,
    people: bool,
) -> Result<discovery::Search, LaunchError> {
    let root = state.data_root.clone();
    let chosen = identity::load(&root).ok().and_then(|store| {
        store
            .default
            .or_else(|| store.accounts.first().map(|a| a.user_id))
    });
    let cookie = match chosen {
        Some(user) => identity::cookie_for(&root, user).ok(),
        None => None,
    };

    let budget = state.budget.clone();
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        discovery::search(&budget, &cache, cookie.as_deref(), &term, people)
    })
    .await
    .map_err(|e| worker_failed(format!("The search was not answered: {e}")))?
}

/// One game, for the detail page.
#[tauri::command]
async fn discovery_game(
    state: State<'_, AppState>,
    universe: u64,
) -> Result<discovery::GameDetail, LaunchError> {
    let root = state.data_root.clone();
    let cookie = identity::load(&root).ok().and_then(|store| {
        let user = store
            .default
            .or_else(|| store.accounts.first().map(|a| a.user_id))?;
        identity::cookie_for(&root, user).ok()
    });
    let budget = state.budget.clone();
    let cache = state.cache.clone();

    tauri::async_runtime::spawn_blocking(move || {
        discovery::game(&budget, &cache, cookie.as_deref(), universe)
    })
    .await
    .map_err(|e| worker_failed(format!("That game was not read: {e}")))?
}

/// The public server list. Three a minute, so only a deliberate ask reaches it.
#[tauri::command]
async fn discovery_servers(
    state: State<'_, AppState>,
    place: u64,
) -> Result<discovery::Servers, LaunchError> {
    let budget = state.budget.clone();
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || discovery::servers(&budget, &cache, place))
        .await
        .map_err(|e| worker_failed(format!("The server list was not read: {e}")))?
}

/// One page of the account's favourites, hydrated into the same cards every other
/// grid draws. Favourites are public, so this does not need the session.
#[tauri::command]
async fn library_favorites(
    state: State<'_, AppState>,
    user_id: u64,
    cursor: Option<String>,
) -> Result<discovery::Shelf, LaunchError> {
    let budget = state.budget.clone();
    let cache = state.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        discovery::favorites(&budget, &cache, user_id, cursor.as_deref())
    })
    .await
    .map_err(|e| worker_failed(format!("Your favourites were not read: {e}")))?
}

/// What this machine launched, newest first.
#[tauri::command]
fn library_recent(state: State<'_, AppState>) -> library::Log {
    library::load(&state.data_root)
}

/// Records a launch that actually started. Called after `launch_start` answers,
/// because the page is what knows which game a place belongs to.
#[tauri::command]
fn library_record(
    state: State<'_, AppState>,
    entry: library::Played,
) -> Result<library::Log, LaunchError> {
    library::record(&state.data_root, entry)
}

#[tauri::command]
fn library_clear(state: State<'_, AppState>) -> Result<library::Log, LaunchError> {
    library::clear(&state.data_root)
}

/// The friend list with names, faces and live presence.
///
/// Needs the session for the list itself; presence is anonymous. The list is cached
/// for two minutes because it allows twenty calls a minute, and presence is not cached
/// because it is the only part that changes.
#[tauri::command]
async fn friends_roster(state: State<'_, AppState>) -> Result<social::Roster, LaunchError> {
    let root = state.data_root.clone();
    let chosen = identity::load(&root)
        .ok()
        .and_then(|store| {
            store
                .default
                .or_else(|| store.accounts.first().map(|a| a.user_id))
        })
        .ok_or_else(|| {
            LaunchError::new(
                FailureCode::SessionExpired,
                "The friend list belongs to an account and this machine has none.",
            )
            .with_action("Add an account")
        })?;

    let cookie = identity::cookie_for(&root, chosen)?;
    let budget = state.budget.clone();
    let cache = state.cache.clone();

    tauri::async_runtime::spawn_blocking(move || social::roster(&budget, &cache, &cookie, chosen))
        .await
        .map_err(|e| worker_failed(format!("The friend list was not read: {e}")))?
}

/// What is left of each allowance, so a screen can say why a button is waiting.
#[tauri::command]
fn discovery_budget(state: State<'_, AppState>) -> Vec<(String, usize, usize)> {
    state.budget.head_room(&[
        discovery::Class::Feed,
        discovery::Class::Search,
        discovery::Class::Games,
        discovery::Class::Servers,
        discovery::Class::Thumbs,
        discovery::Class::Library,
        discovery::Class::Friends,
        discovery::Class::Presence,
    ])
}

#[tauri::command]
fn flags_export(state: State<'_, AppState>, name: String) -> Result<String, LaunchError> {
    let store = flags::store::load(&state.data_root)?;
    let profile = flags::store::profile_named(&store, &name).ok_or_else(|| {
        LaunchError::new(
            FailureCode::NotFound,
            format!("There is no profile called {name}."),
        )
    })?;
    let file = flags::export(&state.data_root, profile)?;
    Ok(file.to_string_lossy().to_string())
}

#[tauri::command]
fn flags_import(path: String) -> Result<flags::Imported, LaunchError> {
    flags::import(std::path::Path::new(&path))
}

/// Opens the Roblox versions folder, so a person can see what the screen is
/// describing without being handed a path to paste somewhere.
#[tauri::command]
fn open_versions_root() -> Result<(), LaunchError> {
    let root = supply::versions_root()?;
    std::process::Command::new("explorer")
        .arg(&root)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            LaunchError::new(
                FailureCode::Internal,
                format!("Explorer could not open the versions folder: {e}"),
            )
        })
}

/// Finds one client folder in whichever root holds it.
fn locate(folder: &str) -> Result<std::path::PathBuf, LaunchError> {
    for root in supply::roots() {
        let candidate = std::path::PathBuf::from(&root.path).join(folder);
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    Err(LaunchError::new(
        FailureCode::ClientMissing,
        format!("{folder} is no longer in any versions folder."),
    ))
}

fn pinned_now(state: &State<'_, AppState>) -> Result<Option<String>, LaunchError> {
    Ok(state
        .settings
        .lock()
        .map_err(|_| poisoned())?
        .supply
        .pinned
        .clone())
}

/// Everything the versions screen needs, read from disk. No network.
#[tauri::command]
fn supply_snapshot(state: State<'_, AppState>) -> Result<Snapshot, LaunchError> {
    let pinned = pinned_now(&state)?;
    supply::snapshot(&state.supply, &pinned)
}

/// Asks Roblox what is LIVE for both clients, then returns a fresh snapshot.
///
/// A failure on the Player lookup fails the call, because that is the client a
/// person actually launches. A Studio failure must not hide a working Player
/// answer, so it is dropped rather than surfaced.
#[tauri::command]
async fn supply_refresh(state: State<'_, AppState>) -> Result<Snapshot, LaunchError> {
    let supply = state.supply.clone();
    let pinned = pinned_now(&state)?;

    tauri::async_runtime::spawn_blocking(move || {
        let live = supply::resolve(Kind::Player, "LIVE")?;
        supply.remember(Kind::Player, &live);

        if let Ok(studio) = supply::resolve(Kind::Studio, "LIVE") {
            supply.remember(Kind::Studio, &studio);
        }

        supply::snapshot(&supply, &pinned)
    })
    .await
    .map_err(|e| worker_failed(format!("The version check did not finish: {e}")))?
}

/// Installs one deployment and returns the snapshot after it lands.
#[tauri::command]
async fn supply_install(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: Kind,
    upload: String,
    method: Method,
) -> Result<Snapshot, LaunchError> {
    let supply = state.supply.clone();
    let pinned = pinned_now(&state)?;
    supply.begin();

    tauri::async_runtime::spawn_blocking(move || {
        let report = |phase: &str, label: String, done: u64, total: u64| {
            let _ = app.emit(
                EVENT,
                Progress {
                    phase: phase.to_string(),
                    label,
                    done,
                    total,
                },
            );
        };

        supply::install::run(&report, &supply.cancel, kind, &upload, method)?;
        supply::snapshot(&supply, &pinned)
    })
    .await
    .map_err(|e| worker_failed(format!("The install did not finish: {e}")))?
}

#[tauri::command]
fn supply_cancel(state: State<'_, AppState>) {
    state.supply.request_cancel();
}

/// Pins or unpins one client folder. Returns the stored settings so the screen
/// reflects what was actually written, not what was asked for.
#[tauri::command]
fn supply_pin(state: State<'_, AppState>, folder: Option<String>) -> Result<Settings, LaunchError> {
    let mut guard = state.settings.lock().map_err(|_| poisoned())?;
    let mut next = guard.clone();
    next.supply.pinned = folder;
    settings::save(&state.data_root, &next)?;
    *guard = next.clone();
    Ok(next)
}

/// What a prune would remove, without removing anything.
#[tauri::command]
fn supply_prune_plan(state: State<'_, AppState>) -> Result<supply::prune::Plan, LaunchError> {
    let pinned = pinned_now(&state)?;
    let live = state.supply.cached(Kind::Player);
    let installed = supply::discover_all(&pinned, live.as_ref().map(|l| l.upload.as_str()))?;
    Ok(supply::prune::plan(&installed))
}

#[tauri::command]
async fn supply_prune(state: State<'_, AppState>) -> Result<PruneResult, LaunchError> {
    let pinned = pinned_now(&state)?;
    let supply = state.supply.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let live = supply.cached(Kind::Player);
        let installed = supply::discover_all(&pinned, live.as_ref().map(|l| l.upload.as_str()))?;
        Ok(supply::prune::run(&installed, &pinned))
    })
    .await
    .map_err(|e| worker_failed(format!("The prune did not finish: {e}")))?
}

/// Removes exactly one client folder, and refuses the ones that must survive.
///
/// This is separate from the prune on purpose: a button on one row must never
/// be able to delete the other rows, so it takes the folder it was pressed on
/// and nothing else.
#[tauri::command]
async fn supply_remove(
    state: State<'_, AppState>,
    folder: String,
) -> Result<Snapshot, LaunchError> {
    let pinned = pinned_now(&state)?;
    let supply = state.supply.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let path = locate(&folder)?;

        if supply
            .cached(Kind::Player)
            .is_some_and(|live| live.upload == folder)
        {
            return Err(LaunchError::new(
                FailureCode::AlreadyRunning,
                format!("{folder} is the version Roblox is serving, so it was not removed."),
            ));
        }

        supply::prune::remove(&path, &folder, &pinned)?;
        supply::snapshot(&supply, &pinned)
    })
    .await
    .map_err(|e| worker_failed(format!("The removal did not finish: {e}")))?
}

/// Re-hashes a client against the baseline written when Kuro installed it.
#[tauri::command]
async fn supply_verify(folder: String) -> Result<VerifyReport, LaunchError> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = locate(&folder)?;
        if !path.is_dir() {
            return Err(LaunchError::new(
                FailureCode::ClientMissing,
                format!("{folder} is no longer in the versions folder."),
            ));
        }
        supply::install::verify(&path)
    })
    .await
    .map_err(|e| worker_failed(format!("The check did not finish: {e}")))?
}

/// Fetches only the archives that own a damaged file and unpacks them over the
/// existing install. This repairs one folder in place and never touches another.
#[tauri::command]
async fn supply_repair(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
) -> Result<VerifyReport, LaunchError> {
    let supply = state.supply.clone();
    supply.begin();

    tauri::async_runtime::spawn_blocking(move || {
        let path = locate(&folder)?;
        if !path.is_dir() {
            return Err(LaunchError::new(
                FailureCode::ClientMissing,
                format!("{folder} is no longer in the versions folder."),
            ));
        }

        let before = supply::install::verify(&path)?;
        if !before.tracked {
            return Err(LaunchError::new(
                FailureCode::ClientMissing,
                "Kuro did not install this folder, so it has no hashes to repair against.",
            )
            .with_action("Install it again with Kuro"));
        }
        if before.clean() {
            return Ok(before);
        }

        let holds_a_client = [Kind::Player, Kind::Studio]
            .into_iter()
            .any(|kind| path.join(kind.executable()).is_file());
        if !holds_a_client {
            return Err(LaunchError::new(
                FailureCode::ClientMissing,
                format!("{folder} no longer holds a client."),
            ));
        }

        let report = |phase: &str, label: String, done: u64, total: u64| {
            let _ = app.emit(
                EVENT,
                Progress {
                    phase: phase.to_string(),
                    label,
                    done,
                    total,
                },
            );
        };

        supply::install::repair(&report, &supply.cancel, &folder, &before.packages)?;
        supply::install::verify(&path)
    })
    .await
    .map_err(|e| worker_failed(format!("The repair did not finish: {e}")))?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        // Size and position are restored; visibility is not. The plugin saves
        // `visible: true` with the geometry, and restoring that showed this
        // window at its full size for a frame before the opening shrank it,
        // which is exactly the flash the hidden start exists to prevent.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .setup(|app| {
            let root = settings::data_root()?;
            std::fs::create_dir_all(&root)?;

            // A settings file that cannot be read must not stop the app from
            // starting. The defaults take over and the UI is told why, so the
            // person still has a way out.
            let (loaded, settings_error) = match settings::load(&root) {
                Ok(settings) => (settings, None),
                Err(error) => {
                    eprintln!("settings could not be read: {error}");
                    (Settings::default(), Some(error))
                }
            };

            // A crash mid-install leaves a staging folder behind. Clearing it
            // here means the next install starts from a known state.
            if let Ok(versions) = supply::versions_root() {
                if versions.is_dir() {
                    supply::install::sweep(&versions);
                }
            }

            // The window is created hidden and stays hidden until the page asks
            // for it, which is what keeps a full-size frame from flashing before
            // the opening scene is ready.
            //
            // The safety net matters more than it looks: if the page never asks,
            // whether from a script error or a webview that failed to start, an
            // app that shows no window at all is unrecoverable. Three seconds
            // later the window appears at its default size instead.
            let opened = Arc::new(AtomicBool::new(false));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unmaximize();
                let flag = opened.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    if !flag.load(Ordering::SeqCst) {
                        let _ = window.show();
                        let _ = window.center();
                    }
                });
            }

            // The switch may already be on from a previous run, and a launch
            // right after start must find the mutex already held.
            let singleton = if loaded.launch.multi_instance {
                launch::spawn::hold_singleton().ok()
            } else {
                None
            };

            // A lease left behind means the last run ended before it could put
            // the client's own settings back. Restoring here is what makes the
            // crash path real rather than aspirational.
            let recovered = flags::apply::revert(&root).ok().flatten();

            app.manage(AppState {
                data_root: root,
                settings: Mutex::new(loaded),
                settings_error,
                singleton: Mutex::new(singleton),
                recovered: Mutex::new(recovered),
                supply: SupplyState::default(),
                budget: std::sync::Arc::new(discovery::Budget::default()),
                cache: std::sync::Arc::new(discovery::Cache::default()),
                opened,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_load,
            settings_save,
            settings_reset,
            open_data_root,
            open_versions_root,
            open_site,
            accounts_list,
            accounts_import,
            accounts_check,
            accounts_remove,
            accounts_default,
            quicklogin_start,
            quicklogin_poll,
            quicklogin_cancel,
            quicklogin_finish,
            launch_read,
            launch_start,
            launch_multi_instance,
            launch_state,
            discovery_home,
            discovery_search,
            discovery_game,
            discovery_servers,
            discovery_budget,
            library_favorites,
            library_recent,
            library_record,
            library_clear,
            friends_roster,
            flags_view,
            flags_refresh,
            flags_search,
            flags_save_profile,
            flags_delete_profile,
            flags_set_scope,
            flags_resolve,
            flags_apply,
            flags_revert,
            flags_export,
            flags_import,
            window_open,
            window_grow,
            supply_snapshot,
            supply_refresh,
            supply_install,
            supply_cancel,
            supply_pin,
            supply_prune_plan,
            supply_prune,
            supply_verify,
            supply_repair,
            supply_remove
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window failed to start");
}
