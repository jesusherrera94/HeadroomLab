// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::rc::Rc;
use std::cell::RefCell;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;

    let secondary_window: Rc<RefCell<Option<HelloWindow>>> = Rc::new(RefCell::new(None));

    ui.on_request_increase_value({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            ui.set_counter(ui.get_counter() + 1);
        }
    });

    ui.on_open_new_window({
        let secondary_window = secondary_window.clone();
        move || {
            let hello = HelloWindow::new().unwrap();
            hello.show().unwrap();
            *secondary_window.borrow_mut() = Some(hello);
        }
    });

    ui.run()?;

    Ok(())
}
