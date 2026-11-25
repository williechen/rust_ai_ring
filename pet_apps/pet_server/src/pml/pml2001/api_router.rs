use crate::pet_store::PetStore;
use pet_application::{
    idle::Schedule,
    idle_dto::{IdleAction, IdleStatus},
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};

struct Entry {
    started: Instant,
    token: uuid::Uuid,
    model: Schedule,
    #[cfg(feature = "e2e-tests")]
    offset: u64,
}

impl Entry {
    fn now(&self) -> u64 {
        #[cfg(feature = "e2e-tests")]
        if std::env::var("RING_TEST_CLOCK").ok().as_deref() == Some("1") {
            return self.offset;
        }
        self.started.elapsed().as_secs()
    }
}

fn entries() -> &'static Mutex<HashMap<String, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(Mutex::default)
}

pub(crate) fn remove(id: &str) {
    if let Ok(mut all) = entries().lock() {
        all.remove(id);
    }
}

pub(crate) fn walking(id: &str) -> bool {
    entries().lock().is_ok_and(|all| {
        all.get(id)
            .is_some_and(|e| e.model.action == IdleAction::Walk)
    })
}

fn status(id: &str) -> Result<IdleStatus, String> {
    let all = entries().lock().map_err(|_| "自然活動狀態無法使用")?;
    Ok(match all.get(id) {
        Some(e) => IdleStatus {
            pet_id: id.into(),
            outside: true,
            action: Some(e.model.action),
            selection_revision: e.model.selections,
            next_in_seconds: Some(e.model.deadline.saturating_sub(e.now())),
        },
        None => IdleStatus {
            pet_id: id.into(),
            outside: false,
            action: None,
            selection_revision: 0,
            next_in_seconds: None,
        },
    })
}

fn emit(app: &tauri::AppHandle, id: &str) {
    if let Ok(state) = status(id) {
        let _ = app.emit("pet-idle-changed", state);
    }
}

fn apply(app: &tauri::AppHandle, id: &str, action: IdleAction) -> Result<(), String> {
    use pet_domain::{PetCommand, PetState};
    let runtime = app.state::<crate::pet_windows::Runtime>();
    let state = runtime.snapshot(id)?.state;
    if state == PetState::Sleeping && action != IdleAction::Sleep {
        crate::pet_windows::dispatch_and_emit(app, id, PetCommand::Wake)?;
    }
    if state == PetState::Interacting {
        crate::pet_windows::dispatch_and_emit(app, id, PetCommand::FinishInteraction)?;
    }
    if action == IdleAction::Sleep && runtime.snapshot(id)?.state != PetState::Sleeping {
        crate::pet_windows::dispatch_and_emit(app, id, PetCommand::Sleep)?;
    }
    emit(app, id);
    Ok(())
}

pub(crate) fn start(app: tauri::AppHandle, id: &str) {
    let token = uuid::Uuid::new_v4();
    {
        let Ok(mut all) = entries().lock() else {
            return;
        };
        if all.contains_key(id) {
            return;
        }
        all.insert(
            id.into(),
            Entry {
                started: Instant::now(),
                token,
                model: Schedule::new(0),
                #[cfg(feature = "e2e-tests")]
                offset: 0,
            },
        );
    }
    let id = id.to_owned();
    emit(&app, &id);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let alive = entries()
                .lock()
                .is_ok_and(|all| all.get(&id).is_some_and(|e| e.token == token));
            if !alive {
                break;
            }
            if let Err(error) = poll(&app, &id).await {
                eprintln!("自然活動暫停：{error}");
            }
        }
    });
}

async fn poll(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let _operation = crate::pet_windows::lifecycle_guard().await;
    if app
        .get_webview_window(&crate::pet_windows::label(id)?)
        .is_none()
    {
        remove(id);
        return Ok(());
    }
    let due = entries()
        .lock()
        .map_err(|_| "自然活動鎖無法使用")?
        .get(id)
        .is_some_and(|e| e.now() >= e.model.deadline);
    if !due {
        return Ok(());
    }
    let store = app.state::<PetStore>();
    let eligible = store.activity_status(id).await?;
    let selected = {
        let mut all = entries().lock().map_err(|_| "自然活動鎖無法使用")?;
        all.get_mut(id).and_then(|e| {
            e.model.poll(
                e.now(),
                eligible.roaming,
                eligible.sitting,
                uuid::Uuid::new_v4().as_u128(),
                uuid::Uuid::new_v4().as_u128(),
            )
        })
    };
    if let Some(action) = selected {
        apply(app, id, action)?;
    }
    Ok(())
}

pub(crate) fn response(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let exists = {
        let mut all = entries().lock().map_err(|_| "自然活動鎖無法使用")?;
        if let Some(e) = all.get_mut(id) {
            e.model.response(e.now());
            true
        } else {
            false
        }
    };
    if exists {
        apply(app, id, IdleAction::Idle)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn pet_idle_status(
    id: String,
    store: State<'_, PetStore>,
) -> Result<IdleStatus, String> {
    store.profile(&id).await?;
    status(&id)
}

#[tauri::command]
pub(crate) async fn set_pet_idle_choice(
    id: String,
    choice: String,
    app: tauri::AppHandle,
    store: State<'_, PetStore>,
) -> Result<pet_persistence::ActivityStatus, String> {
    let _operation = crate::pet_windows::lifecycle_guard().await;
    pet_persistence::pml::pml2001::sql_mapper::configure(&store.pool, &id, &choice).await?;
    super_status(&app, &store, &id).await
}

pub(crate) async fn super_status(
    app: &tauri::AppHandle,
    store: &PetStore,
    id: &str,
) -> Result<pet_persistence::ActivityStatus, String> {
    let mut state = store.activity_status(id).await?;
    state.outside = app
        .get_webview_window(&crate::pet_windows::label(id)?)
        .is_some();
    Ok(state)
}

#[cfg(feature = "e2e-tests")]
#[tauri::command]
pub(crate) async fn advance_pet_idle_clock(
    id: String,
    seconds: u64,
    app: tauri::AppHandle,
    store: State<'_, PetStore>,
) -> Result<IdleStatus, String> {
    if std::env::var("RING_TEST_CLOCK").ok().as_deref() != Some("1") {
        return Err("測試時鐘未啟用".into());
    }
    store.profile(&id).await?;
    {
        let mut all = entries().lock().map_err(|_| "自然活動鎖無法使用")?;
        if let Some(e) = all.get_mut(&id) {
            e.offset = e.offset.saturating_add(seconds);
        }
    }
    poll(&app, &id).await?;
    status(&id)
}
