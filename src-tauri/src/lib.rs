use alltokens_core::model::{
    BudgetConfig, CodexQuotaWindow, Pagination, RequestFilter,
};
use alltokens_core::pricing::PricingEngine;
use alltokens_core::storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry,
};
use tauri_plugin_notification::NotificationExt;

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
use tauri_plugin_autostart::ManagerExt;

static STORAGE: OnceLock<Storage> = OnceLock::new();
static QUOTA_HEADER_ITEM: OnceLock<MenuItem<Wry>> = OnceLock::new();
static WIDGET_TOGGLE_ITEM: OnceLock<CheckMenuItem<Wry>> = OnceLock::new();

const BUDGET_ALERT_STATE_KEY: &str = "budget_alert_state";
const TRAY_ID: &str = "main-tray";
const TRAY_QUOTA_REFRESH_SECS: u64 = 120;
const WIDGET_LABEL: &str = "widget";
const WIDGET_WIDTH: f64 = 320.0;
const WIDGET_HEIGHT: f64 = 480.0;

#[derive(Debug, Default, Serialize, Deserialize)]
struct BudgetAlertState {
    month: String,
    warned_80: bool,
    warned_100: bool,
}

fn get_storage() -> &'static Storage {
    STORAGE.get().expect("Storage not initialized")
}

fn load_pricing() -> Result<PricingEngine, String> {
    get_storage()
        .load_pricing_engine()
        .map_err(|e| e.to_string())
}

fn current_month_key() -> String {
    chrono::Utc::now().format("%Y-%m").to_string()
}

fn month_start_iso() -> String {
    chrono::Utc::now().format("%Y-%m-01T00:00:00Z").to_string()
}

fn load_budget_alert_state(storage: &Storage) -> Result<BudgetAlertState, String> {
    let month = current_month_key();
    match storage.get_config(BUDGET_ALERT_STATE_KEY).map_err(|e| e.to_string())? {
        Some(raw) => {
            let mut state: BudgetAlertState =
                serde_json::from_str(&raw).unwrap_or_default();
            if state.month != month {
                state = BudgetAlertState {
                    month,
                    ..Default::default()
                };
            }
            Ok(state)
        }
        None => Ok(BudgetAlertState {
            month,
            ..Default::default()
        }),
    }
}

fn save_budget_alert_state(storage: &Storage, state: &BudgetAlertState) -> Result<(), String> {
    let raw = serde_json::to_string(state).map_err(|e| e.to_string())?;
    storage
        .set_config(BUDGET_ALERT_STATE_KEY, &raw)
        .map_err(|e| e.to_string())
}

fn monthly_cost_usd(storage: &Storage) -> Result<f64, String> {
    let filter = RequestFilter {
        start_date: Some(month_start_iso()),
        ..Default::default()
    };
    storage
        .get_overview(&filter)
        .map(|stats| stats.total_cost_usd)
        .map_err(|e| e.to_string())
}

pub fn check_budget_alerts(app: &tauri::AppHandle) -> Result<(), String> {
    let storage = get_storage();
    let config: BudgetConfig = storage
        .get_budget_config()
        .map_err(|e| e.to_string())?;

    if !config.enabled {
        return Ok(());
    }

    let budget = match config.monthly_usd {
        Some(amount) if amount > 0.0 => amount,
        _ => return Ok(()),
    };

    let spent = monthly_cost_usd(storage)?;
    let ratio = spent / budget;
    let mut state = load_budget_alert_state(storage)?;

    if ratio >= 1.0 && !state.warned_100 {
        app.notification()
            .builder()
            .title("AllTokens — Budget Exceeded")
            .body(format!(
                "${spent:.2} of ${budget:.2} monthly budget used ({:.0}%)",
                ratio * 100.0
            ))
            .show()
            .map_err(|e| e.to_string())?;
        state.warned_100 = true;
        state.warned_80 = true;
        save_budget_alert_state(storage, &state)?;
    } else if ratio >= 0.8 && !state.warned_80 {
        app.notification()
            .builder()
            .title("AllTokens — Budget Warning")
            .body(format!(
                "${spent:.2} of ${budget:.2} monthly budget used ({:.0}%)",
                ratio * 100.0
            ))
            .show()
            .map_err(|e| e.to_string())?;
        state.warned_80 = true;
        save_budget_alert_state(storage, &state)?;
    }

    Ok(())
}

fn window_remaining_percent(window: &Option<CodexQuotaWindow>) -> Option<i32> {
    let w = window.as_ref()?;
    w.remaining_percent
        .or_else(|| w.used_percent.map(|used| 100 - used))
}

fn format_quota_windows(
    five: &Option<CodexQuotaWindow>,
    seven: &Option<CodexQuotaWindow>,
) -> Option<String> {
    let five_pct = window_remaining_percent(five);
    let seven_pct = window_remaining_percent(seven);
    match (five_pct, seven_pct) {
        (Some(f), Some(s)) => Some(format!("5h: {f}% | 7d: {s}%")),
        (Some(f), None) => Some(format!("5h: {f}%")),
        (None, Some(s)) => Some(format!("7d: {s}%")),
        (None, None) => None,
    }
}

struct TrayQuotaSummary {
    tooltip: String,
    menu_header: String,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    tray_title: Option<String>,
}

fn build_tray_quota_summary(storage: &Storage) -> TrayQuotaSummary {
    let codex = storage.get_codex_quota_snapshot().ok().flatten();
    let claude = storage.get_claude_quota_snapshot().ok().flatten();

    let codex_part = codex.as_ref().and_then(|snapshot| {
        format_quota_windows(&snapshot.five_hour, &snapshot.seven_day)
            .map(|windows| format!("Codex {windows}"))
    });
    let claude_part = claude.as_ref().and_then(|snapshot| {
        format_quota_windows(&snapshot.five_hour, &snapshot.seven_day)
            .map(|windows| format!("Claude {windows}"))
    });

    let parts: Vec<String> = [codex_part, claude_part].into_iter().flatten().collect();
    let has_quota = !parts.is_empty();

    let summary_line = if has_quota {
        parts.join(" · ")
    } else {
        "Quota: open dashboard to refresh".to_string()
    };

    let tooltip = if has_quota {
        format!("AllTokens — {summary_line}")
    } else {
        "AllTokens".to_string()
    };

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let tray_title = {
        let codex_5h = codex
            .as_ref()
            .and_then(|snapshot| window_remaining_percent(&snapshot.five_hour));
        let claude_5h = claude
            .as_ref()
            .and_then(|snapshot| window_remaining_percent(&snapshot.five_hour));
        match (codex_5h, claude_5h) {
            (Some(c), Some(l)) => Some(format!("C:{c}% L:{l}%")),
            (Some(c), None) => Some(format!("Codex {c}%")),
            (None, Some(l)) => Some(format!("Claude {l}%")),
            (None, None) => None,
        }
    };

    TrayQuotaSummary {
        tooltip,
        menu_header: summary_line,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        tray_title,
    }
}

pub fn update_tray_quota_display(app: &tauri::AppHandle) -> Result<(), String> {
    let summary = build_tray_quota_summary(get_storage());

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_tooltip(Some(&summary.tooltip))
            .map_err(|e| e.to_string())?;

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            if let Some(title) = &summary.tray_title {
                tray.set_title(Some(title)).map_err(|e| e.to_string())?;
            } else {
                tray.set_title(None::<&str>).map_err(|e| e.to_string())?;
            }
        }
    }

    if let Some(header) = QUOTA_HEADER_ITEM.get() {
        header
            .set_text(&summary.menu_header)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn start_tray_quota_monitor(app: tauri::AppHandle) {
    let app_for_events = app.clone();
    std::thread::spawn(move || {
        loop {
            if let Some(bus) = alltokens_web::event_bus() {
                let mut rx = bus.subscribe();
                loop {
                    match rx.blocking_recv() {
                        Ok(alltokens_web::WsEvent::ScanComplete { .. }) => {
                            if let Err(e) = update_tray_quota_display(&app_for_events) {
                                eprintln!("Tray quota update after scan failed: {e}");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(TRAY_QUOTA_REFRESH_SECS));
            if let Err(e) = update_tray_quota_display(&app) {
                eprintln!("Periodic tray quota update failed: {e}");
            }
        }
    });
}

fn start_budget_monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(TRAY_QUOTA_REFRESH_SECS));
            if let Err(e) = check_budget_alerts(&app) {
                eprintln!("Budget alert check failed: {e}");
            }
        }
    });
}

fn start_auto_scan_monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last_autostart: Option<bool> = None;
        loop {
            let interval_mins = get_storage()
                .get_general_config()
                .map(|c| c.auto_scan_interval_minutes)
                .unwrap_or(0);

            if let Ok(config) = get_storage().get_general_config() {
                if last_autostart != Some(config.launch_at_startup) {
                    if let Err(e) = apply_launch_at_startup(&app, config.launch_at_startup) {
                        eprintln!("Autostart sync failed: {e}");
                    }
                    last_autostart = Some(config.launch_at_startup);
                }
            }

            if interval_mins == 0 {
                std::thread::sleep(std::time::Duration::from_secs(60));
                continue;
            }

            std::thread::sleep(std::time::Duration::from_secs(interval_mins as u64 * 60));

            let pricing = match load_pricing() {
                Ok(engine) => engine,
                Err(e) => {
                    eprintln!("Auto-scan: failed to load pricing: {e}");
                    continue;
                }
            };

            let rt = tokio::runtime::Runtime::new().unwrap();
            match rt.block_on(alltokens_web::run_scan(get_storage(), &pricing)) {
                Ok(result) => {
                    if let Some(bus) = alltokens_web::event_bus() {
                        bus.emit_scan_complete(result.total);
                    }
                    if let Err(e) = check_budget_alerts(&app) {
                        eprintln!("Budget alert check after auto-scan failed: {e}");
                    }
                    if let Err(e) = update_tray_quota_display(&app) {
                        eprintln!("Tray quota update after auto-scan failed: {e}");
                    }
                }
                Err(e) => eprintln!("Auto-scan failed: {e}"),
            }
        }
    });
}

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
fn apply_launch_at_startup(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn apply_launch_at_startup(_app: &tauri::AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(true) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

// --- 桌面悬浮小组件窗口 (Phase 4 P2) ---

fn load_widget_config() -> alltokens_core::model::WidgetConfig {
    get_storage().get_widget_config().unwrap_or_default()
}

fn save_widget_visible(visible: bool) {
    let mut config = load_widget_config();
    config.visible = visible;
    if let Err(e) = get_storage().set_widget_config(&config) {
        eprintln!("Widget config save failed: {e}");
    }
}

fn save_widget_position(x: i32, y: i32) {
    let mut config = load_widget_config();
    config.x = Some(x);
    config.y = Some(y);
    if let Err(e) = get_storage().set_widget_config(&config) {
        eprintln!("Widget position save failed: {e}");
    }
}

fn sync_widget_toggle(visible: bool) {
    if let Some(item) = WIDGET_TOGGLE_ITEM.get() {
        let _ = item.set_checked(visible);
    }
}

/// Create the floating widget window (hidden initially), restoring the last
/// saved position when available.
fn create_widget_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    let config = load_widget_config();
    let window = WebviewWindowBuilder::new(
        app,
        WIDGET_LABEL,
        WebviewUrl::App("index.html?widget=1".into()),
    )
    .title("AllTokens")
    .inner_size(WIDGET_WIDTH, WIDGET_HEIGHT)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .build()
    .map_err(|e| e.to_string())?;

    if let (Some(x), Some(y)) = (config.x, config.y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    Ok(window)
}

/// Show or hide the widget window (creating it on first use), persist the
/// visibility flag, and keep the tray checkbox in sync.
fn apply_widget_visibility(app: &tauri::AppHandle, visible: bool) -> Result<(), String> {
    if visible {
        let window = match app.get_webview_window(WIDGET_LABEL) {
            Some(w) => w,
            None => create_widget_window(app)?,
        };
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else if let Some(window) = app.get_webview_window(WIDGET_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    save_widget_visible(visible);
    sync_widget_toggle(visible);
    Ok(())
}

fn toggle_widget(app: &tauri::AppHandle) {
    let next = !load_widget_config().visible;
    if let Err(e) = apply_widget_visibility(app, next) {
        eprintln!("Widget toggle failed: {e}");
    }
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let quota_header = MenuItem::with_id(
        app,
        "quota_header",
        "Quota: open dashboard to refresh",
        false,
        None::<&str>,
    )?;
    let _ = QUOTA_HEADER_ITEM.set(quota_header.clone());
    let separator = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "Show AllTokens", true, None::<&str>)?;
    let widget_i = CheckMenuItem::with_id(
        app,
        "toggle_widget",
        "桌面小组件",
        true,
        load_widget_config().visible,
        None::<&str>,
    )?;
    let _ = WIDGET_TOGGLE_ITEM.set(widget_i.clone());
    let hide_i = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&quota_header, &separator, &show_i, &widget_i, &hide_i, &quit_i],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("AllTokens")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "toggle_widget" => toggle_widget(app),
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    if let Err(e) = update_tray_quota_display(app) {
        eprintln!("Initial tray quota update failed: {e}");
    }
    Ok(())
}

#[tauri::command]
fn get_overview(last: Option<String>) -> Result<alltokens_core::model::OverviewStats, String> {
    let filter = RequestFilter {
        start_date: last.map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }),
        ..Default::default()
    };
    get_storage().get_overview(&filter).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_providers(last: Option<String>) -> Result<Vec<alltokens_core::model::ProviderStats>, String> {
    let filter = RequestFilter {
        start_date: last.map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }),
        ..Default::default()
    };
    get_storage()
        .get_provider_stats(&filter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_models(last: Option<String>) -> Result<Vec<alltokens_core::model::ModelStats>, String> {
    let filter = RequestFilter {
        start_date: last.map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }),
        ..Default::default()
    };
    get_storage().get_model_stats(&filter).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_trends(last: Option<String>) -> Result<Vec<alltokens_core::model::DailySummary>, String> {
    let filter = RequestFilter {
        start_date: last.map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }),
        ..Default::default()
    };
    get_storage()
        .get_daily_trends(&filter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_requests(
    last: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<
    alltokens_core::model::PaginatedResult<alltokens_core::model::UsageRecord>,
    String,
> {
    let filter = RequestFilter {
        start_date: last.map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        }),
        ..Default::default()
    };
    let pg = Pagination {
        page: page.unwrap_or(0),
        page_size: page_size.unwrap_or(50),
    };
    get_storage()
        .get_requests(&filter, &pg)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_scan(app: tauri::AppHandle) -> Result<alltokens_web::ScanResult, String> {
    let pricing = load_pricing()?;
    let result = alltokens_web::run_scan(get_storage(), &pricing)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(bus) = alltokens_web::event_bus() {
        bus.emit_scan_complete(result.total);
    }
    if let Err(e) = check_budget_alerts(&app) {
        eprintln!("Budget alert check after scan failed: {e}");
    }
    if let Err(e) = update_tray_quota_display(&app) {
        eprintln!("Tray quota update after scan failed: {e}");
    }
    Ok(result)
}

#[tauri::command]
fn set_widget_visible(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    apply_widget_visibility(&app, visible)
}

#[tauri::command]
fn open_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tray_quota_tests {
    use super::*;
    use alltokens_core::model::{
        ClaudeQuotaSnapshot, CodexQuotaSnapshot, CodexQuotaWindowKind,
    };
    use chrono::Utc;

    fn sample_window(kind: CodexQuotaWindowKind, remaining: i32) -> CodexQuotaWindow {
        CodexQuotaWindow {
            kind,
            used_percent: Some(100 - remaining),
            remaining_percent: Some(remaining),
            window_duration_mins: None,
            resets_at: None,
        }
    }

    #[test]
    fn format_quota_windows_shows_dual_ring_summary() {
        let five = Some(sample_window(CodexQuotaWindowKind::FiveHour, 72));
        let seven = Some(sample_window(CodexQuotaWindowKind::SevenDay, 45));
        assert_eq!(
            format_quota_windows(&five, &seven).as_deref(),
            Some("5h: 72% | 7d: 45%")
        );
    }

    #[test]
    fn build_tray_quota_summary_uses_cached_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("data.db")).unwrap();
        storage
            .set_codex_quota_snapshot(&CodexQuotaSnapshot {
                fetched_at: Utc::now(),
                source: "test".to_string(),
                plan_type: None,
                five_hour: Some(sample_window(CodexQuotaWindowKind::FiveHour, 72)),
                seven_day: Some(sample_window(CodexQuotaWindowKind::SevenDay, 45)),
                rate_limit_reached: false,
            })
            .unwrap();
        storage
            .set_claude_quota_snapshot(&ClaudeQuotaSnapshot {
                fetched_at: Utc::now(),
                source: "test".to_string(),
                snapshot_path: None,
                captured_at: None,
                is_stale: false,
                five_hour: Some(sample_window(CodexQuotaWindowKind::FiveHour, 68)),
                seven_day: Some(sample_window(CodexQuotaWindowKind::SevenDay, 41)),
            })
            .unwrap();

        let summary = build_tray_quota_summary(&storage);
        assert!(summary.tooltip.contains("Codex 5h: 72% | 7d: 45%"));
        assert!(summary.tooltip.contains("Claude 5h: 68% | 7d: 41%"));
        assert!(summary.menu_header.contains("Codex 5h: 72% | 7d: 45%"));
        assert!(summary.menu_header.contains("Claude 5h: 68% | 7d: 41%"));
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert_eq!(summary.tray_title.as_deref(), Some("C:72% L:68%"));
    }

    #[test]
    fn build_tray_quota_summary_without_cache_uses_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("data.db")).unwrap();
        let summary = build_tray_quota_summary(&storage);
        assert_eq!(summary.tooltip, "AllTokens");
        assert_eq!(summary.menu_header, "Quota: open dashboard to refresh");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init());

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    if window.label() == WIDGET_LABEL {
                        save_widget_visible(false);
                        sync_widget_toggle(false);
                    }
                }
                // 小组件拖动后持久化位置，下次启动还原
                tauri::WindowEvent::Moved(position) if window.label() == WIDGET_LABEL => {
                    save_widget_position(position.x, position.y);
                }
                _ => {}
            }
        })
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&app_dir).ok();
            let db_path = app_dir.join("data.db");
            let storage = Storage::open(&db_path).expect("Failed to open database");

            STORAGE.set(storage).ok();

            let storage_clone = get_storage().clone();
            let events = alltokens_web::EventBus::new();
            let port = 3212u16;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let config = alltokens_web::WebConfig::new(
                        ([127, 0, 0, 1], port).into(),
                        storage_clone,
                    )
                    .with_events(events);
                    if let Err(e) = alltokens_web::start_web(config).await {
                        eprintln!("Web server error: {e}");
                    }
                });
            });

            setup_tray(app.handle())?;
            start_tray_quota_monitor(app.handle().clone());
            start_budget_monitor(app.handle().clone());
            start_auto_scan_monitor(app.handle().clone());

            // 还原小组件显隐状态（位置在创建时从 config 读取）
            if load_widget_config().visible {
                if let Err(e) = apply_widget_visibility(app.handle(), true) {
                    eprintln!("Widget restore failed: {e}");
                }
            }

            if let Ok(config) = get_storage().get_general_config() {
                if let Err(e) = apply_launch_at_startup(app.handle(), config.launch_at_startup) {
                    eprintln!("Initial autostart sync failed: {e}");
                }
            }

            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if let Err(e) = check_budget_alerts(&app_handle) {
                    eprintln!("Initial budget alert check failed: {e}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_overview,
            get_providers,
            get_models,
            get_trends,
            get_requests,
            run_scan,
            set_widget_visible,
            open_main_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
