use libui::controls::{Button, Label, LayoutStrategy, VerticalBox, Window, WindowType};
use libui::prelude::*;

use crate::{gpu, menu, util};

pub fn run() {
    let ui = match UI::init() {
        Ok(ui) => ui,
        Err(e) => {
            util::error_box(&format!("Couldn't initialize UI: {e}"));
            return;
        }
    };

    let title = format!("{} {}", util::APP_NAME, util::VERSION);
    let mut win = Window::new(&ui, &title, 420, 220, WindowType::NoMenubar);
    win.set_margined(true);

    let mut layout = VerticalBox::new();
    layout.set_padded(true);

    let status = Label::new(&status_text());
    let mut install = Button::new("Install Context Menu");
    let mut remove = Button::new("Remove From Context Menu");

    install.on_clicked({
        let win = win.clone();
        let mut status = status.clone();
        move |_| {
            match menu::install() {
                Ok(_) => win.modal_msg(util::APP_NAME, "Context menu installed."),
                Err(e) => win.modal_err(util::APP_NAME, &format!("Install failed: {e:#}")),
            }
            status.set_text(&status_text());
        }
    });

    remove.on_clicked({
        let win = win.clone();
        let mut status = status.clone();
        move |_| {
            match menu::remove() {
                Ok(msg) => win.modal_msg(util::APP_NAME, &msg),
                Err(e) => win.modal_err(util::APP_NAME, &format!("Remove failed: {e:#}")),
            }
            status.set_text(&status_text());
        }
    });

    let hint = Label::new(
        "Tip: on Windows 11, right-click an .exe and choose \"Show more options\",\n\
         or hold Shift while right-clicking, to see \"Run with other graphic\".",
    );

    layout.append(status, LayoutStrategy::Stretchy);
    layout.append(hint, LayoutStrategy::Compact);
    layout.append(install, LayoutStrategy::Compact);
    layout.append(remove, LayoutStrategy::Compact);
    win.set_child(layout);

    win.on_closing(&ui, {
        let ui = ui.clone();
        move |_| ui.quit()
    });
    win.show();
    set_window_icon(&win);
    ui.main();
}

extern "C" {
    // Exported by the statically linked libui-ng; returns the control's HWND.
    fn uiControlHandle(control: *mut std::ffi::c_void) -> usize;
}

/// libui registers its window class with the stock icon; swap in the embedded app icon.
fn set_window_icon(win: &Window) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, WM_SETICON,
    };

    unsafe {
        let hwnd = HWND(uiControlHandle(win.ptr().cast()) as *mut _);
        let Ok(module) = GetModuleHandleW(None) else { return };
        // MAKEINTRESOURCE(1), matching assets/app.rc.
        let id = PCWSTR(1 as *const u16);
        for (kind, size) in [(ICON_BIG, 32), (ICON_SMALL, 16)] {
            if let Ok(icon) = LoadImageW(Some(module.into()), id, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) {
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(icon.0 as isize)));
            }
        }
    }
}

fn status_text() -> String {
    let mut text = format!(
        "Context menu: {}\n\nDetected GPUs:",
        if menu::is_installed() { "installed" } else { "not installed" }
    );
    for g in gpu::detect() {
        text.push_str(&format!("\n  {}", g.menu_label()));
    }
    text
}
