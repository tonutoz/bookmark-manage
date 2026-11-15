//! 트레이 상주. 창을 닫아도 앱은 트레이에 남아 브라우저 확장의 저장 요청을 계속 받는다.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, State, Wry};
use tauri_plugin_autostart::ManagerExt;

/// 로그인할 때 자동 실행되면 이 인자가 붙고, 창 없이 트레이로만 뜬다.
pub const HIDDEN_ARG: &str = "--hidden";

/// 트레이 메뉴의 항목들. 언어를 아는 것은 화면 쪽이라, 화면이 뜬 뒤 글자를 바꿔 준다.
pub struct TrayItems {
    open: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

#[tauri::command]
pub fn set_tray_labels(items: State<TrayItems>, open: String, autostart: String, quit: String) -> Result<(), String> {
    items.open.set_text(open).map_err(|e| e.to_string())?;
    items.autostart.set_text(autostart).map_err(|e| e.to_string())?;
    items.quit.set_text(quit).map_err(|e| e.to_string())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let autostart =
        CheckMenuItem::with_id(app, "autostart", "윈도우 시작 시 실행", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &autostart, &PredefinedMenuItem::separator(app)?, &quit])?;
    app.manage(TrayItems { open: open.clone(), autostart: autostart.clone(), quit: quit.clone() });

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Bookmark Manage")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            "autostart" => {
                // 클릭하면 체크 상태가 먼저 바뀐다. 실제 등록에 실패하면 체크를 되돌린다.
                let wanted = autostart.is_checked().unwrap_or(false);
                let launcher = app.autolaunch();
                let result = if wanted { launcher.enable() } else { launcher.disable() };
                if result.is_err() {
                    let _ = autostart.set_checked(!wanted);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
        show_main(app.handle());
    }
    Ok(())
}
