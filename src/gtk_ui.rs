#[cfg(feature = "gtk-ui")]
use gtk4::{
    gdk::Display,
    glib,
    prelude::*,
    Application, ApplicationWindow, Box as GtkBox, CssProvider, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SearchEntry,
};
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(feature = "gtk-ui")]
use std::sync::{Arc, Mutex};
// Removed unused imports

// Application ID for GTK
#[cfg(feature = "gtk-ui")]
const APP_ID: &str = "org.cyrinux.network_dmenu";

// Function to update the action list
#[cfg(feature = "gtk-ui")]
fn update_action_list(list_box: &ListBox, actions: &[String]) {
    // Clear the list
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    // Add filtered actions
    for action in actions {
        let row = ListBoxRow::new();
        let label = Label::new(Some(action));
        label.set_xalign(0.0);
        label.set_margin_top(10);
        label.set_margin_bottom(10);
        label.set_margin_start(10);
        label.set_margin_end(10);
        row.set_child(Some(&label));
        list_box.append(&row);
    }
}

// Setup CSS styling
#[cfg(feature = "gtk-ui")]
fn setup_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        window { background-color: #292a2e; }
        label { color: #ffffff; font-family: 'monospace'; }
        entry { color: #ffffff; background-color: #3a3b3f; border-radius: 5px; }
        listbox { background-color: #292a2e; }
        listboxrow:selected { background-color: #4a4b4f; }
        "
    );

    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

// Simplified version that uses a more direct approach
pub async fn select_action_with_gtk(actions: Vec<String>) -> Result<Option<String>, Box<dyn std::error::Error>> {
    // Use dmenu as fallback if GTK is not available or not enabled
    #[cfg(not(feature = "gtk-ui"))]
    return use_dmenu_fallback(&actions);

    // For GTK-enabled builds
    #[cfg(feature = "gtk-ui")]
    {
        // Check if GTK is available by trying to load a minimal GTK app
        if gtk4::init().is_err() {
            eprintln!("Failed to initialize GTK, falling back to dmenu");
            return use_dmenu_fallback(&actions);
        }

        // Create a status flag to track selection
        let selected = Arc::new(Mutex::new(None));

        // Create GTK application
        let app = Application::builder()
            .application_id(APP_ID)
            .build();

        let actions_clone = actions.clone();
        let selected_clone = selected.clone();

        app.connect_activate(move |app| {
            // Setup window
            let window = ApplicationWindow::builder()
                .application(app)
                .title("Network Menu")
                .default_width(600)
                .default_height(400)
                .build();

            // Setup CSS
            setup_css();

            // Create main layout
            let main_box = GtkBox::new(Orientation::Vertical, 0);

            // Add search entry
            let search_entry = SearchEntry::new();
            search_entry.set_margin_top(10);
            search_entry.set_margin_bottom(10);
            search_entry.set_margin_start(10);
            search_entry.set_margin_end(10);
            search_entry.set_hexpand(true);

            // Action list with scrolling
            let scrolled = ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .build();

            let action_list = ListBox::new();
            action_list.set_selection_mode(gtk4::SelectionMode::Single);
            action_list.set_activate_on_single_click(true);

            // Populate list
            update_action_list(&action_list, &actions_clone);

            // Connect action selection
            let actions_ref = actions_clone.clone();
            let selected_ref = selected_clone.clone();
            let app_ref = app.clone();

            // Use glib::clone for safer closures
            let row_activated_handler = glib::clone!(@strong actions_ref, @strong selected_ref, @strong app_ref => move |_list: &ListBox, row: &ListBoxRow| {
                let index = row.index();
                if index >= 0 {
                    if let Some(action) = actions_ref.get(index as usize) {
                        *selected_ref.lock().unwrap() = Some(action.clone());
                        app_ref.quit();
                    }
                }
            });

            action_list.connect_row_activated(row_activated_handler);

            // Setup search filtering
            let actions_ref = actions_clone.clone();
            let action_list_ref = action_list.clone();

            // Use glib::clone for safer closures
            let search_changed_handler = glib::clone!(@strong actions_ref, @strong action_list_ref => move |entry: &SearchEntry| {
                let text = entry.text().to_string().to_lowercase();

                let filtered: Vec<String> = if text.is_empty() {
                    actions_ref.clone()
                } else {
                    actions_ref.iter()
                        .filter(|action| action.to_lowercase().contains(&text))
                        .cloned()
                        .collect()
                };

                update_action_list(&action_list_ref, &filtered);
            });

            search_entry.connect_search_changed(search_changed_handler);

            // Assemble UI
            scrolled.set_child(Some(&action_list));
            main_box.append(&search_entry);
            main_box.append(&scrolled);
            window.set_child(Some(&main_box));

            // Focus the search entry
            search_entry.grab_focus();

            // Show window
            window.present();
        });

        // Run application
        let args: Vec<String> = Vec::new();
        let _ = app.run_with_args(&args);

        // After the application has run, check for the selected result
        // Use the original 'selected' here, not a clone that might be moved
        if let Some(result) = selected.lock().unwrap().clone() {
            return Ok(Some(result));
        }

        // If GTK UI didn't return a selection, return None
        return Ok(None);
    }
}

// Helper function to use dmenu as fallback
fn use_dmenu_fallback(actions: &[String]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    eprintln!("Falling back to dmenu");

    // Try using dmenu
    let mut child = match Command::new("dmenu")
        .args(["--no-multi"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn() {
            Ok(child) => child,
            Err(e) => return Err(Box::new(e)),
        };

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        for action in actions {
            if let Err(e) = writeln!(stdin, "{}", action) {
                return Err(Box::new(e));
            }
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(e) => return Err(Box::new(e)),
    };

    if output.status.success() {
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !selected.is_empty() {
            return Ok(Some(selected));
        }
    }

    // If dmenu fails or returns empty, try rofi
    if let Ok(mut child) = Command::new("rofi")
        .args(["-dmenu", "-i", "-p", "Network Menu"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        for action in actions {
            if let Err(e) = writeln!(stdin, "{}", action) {
                return Err(Box::new(e));
            }
        }

        match child.wait_with_output() {
            Ok(output) => {
                if output.status.success() {
                    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !selected.is_empty() {
                        return Ok(Some(selected));
                    }
                }
            },
            Err(e) => return Err(Box::new(e)),
        }
    }

    Ok(None)
}
