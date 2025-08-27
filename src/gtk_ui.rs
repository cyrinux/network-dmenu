#[cfg(feature = "gtk-ui")]
use gtk4::{
    gdk::{Display, Key, ModifierType},
    glib::{self, clone},
    prelude::*,
    Application, ApplicationWindow, Box as GtkBox, CssProvider, EventControllerKey, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SearchEntry, SelectionMode,
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(feature = "gtk-ui")]
use std::sync::{Arc, Mutex};

// Application ID for GTK
#[cfg(feature = "gtk-ui")]
const APP_ID: &str = "org.cyrinux.network_dmenu";

// Function to update the action list with fuzzy search results
#[cfg(feature = "gtk-ui")]
fn update_action_list(list_box: &ListBox, actions: &[String], query: &str, indices: &mut Vec<usize>) {
    // Clear the list
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    // Clear indices
    indices.clear();

    // Create a fuzzy matcher
    let matcher = SkimMatcherV2::default();

    // Find and sort matches
    let mut matches: Vec<(i64, usize, &String, Option<Vec<usize>>)> = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        if let Some(score) = matcher.fuzzy_match(action, query) {
            // Get matched character positions for highlighting
            let match_positions = if !query.is_empty() {
                Some(matcher.fuzzy_indices(action, query).map(|(_score, indices)| indices).unwrap_or_default())
            } else {
                None
            };
            matches.push((score, idx, action, match_positions));
        } else if query.is_empty() {
            // When query is empty, include all with a zero score
            matches.push((0, idx, action, None));
        }
    }

    // Sort by score (higher score = better match)
    matches.sort_by(|a, b| b.0.cmp(&a.0));

    // Add filtered actions
    for (_, idx, action, match_positions) in matches {
        let row = ListBoxRow::new();
        indices.push(idx); // Store the original index in our vector

        let label = Label::new(None);
        label.set_xalign(0.0);
        label.set_margin_top(10);
        label.set_margin_bottom(10);
        label.set_margin_start(10);
        label.set_margin_end(10);

        // Apply text with highlighting if there are matches
        if let Some(positions) = match_positions {
            let mut markup = String::new();
            let chars: Vec<char> = action.chars().collect();

            for (i, c) in chars.iter().enumerate() {
                if positions.contains(&i) {
                    // Highlight matched characters
                    markup.push_str(&format!("<span foreground=\"#f0ad4e\" weight=\"bold\">{}</span>", glib::markup_escape_text(&c.to_string())));
                } else {
                    markup.push_str(&glib::markup_escape_text(&c.to_string()));
                }
            }

            label.set_markup(&markup);
        } else {
            label.set_text(action);
        }

        row.set_child(Some(&label));
        list_box.append(&row);
    }

    // Select the first item if any exists
    if let Some(first_row) = list_box.row_at_index(0) {
        list_box.select_row(Some(&first_row));
    }
}

// Setup CSS styling
#[cfg(feature = "gtk-ui")]
fn setup_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        window {
            background-color: #292a2e;
        }
        label {
            color: #ffffff;
            font-family: 'monospace';
            font-size: 12pt;
        }
        entry {
            color: #ffffff;
            background-color: #3a3b3f;
            border-radius: 5px;
            padding: 8px;
            font-size: 14pt;
            caret-color: #f0ad4e;
        }
        entry:focus {
            border: 1px solid #4d90fe;
        }
        listbox {
            background-color: #292a2e;
        }
        listboxrow {
            padding: 4px;
            transition: background-color 0.1s ease-in-out;
        }
        listboxrow:selected {
            background-color: #4a4b4f;
        }
        listboxrow:hover {
            background-color: #3a3b3f;
        }
        scrolledwindow {
            border: none;
        }
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

        // Vector to track indices mapping
        let indices_arc = Arc::new(Mutex::new(Vec::new()));
        let indices_clone = indices_arc.clone();

        app.connect_activate(clone!(@strong actions_clone, @strong selected_clone, @strong indices_clone => move |app| {
            // Setup window
            let window = ApplicationWindow::builder()
                .application(app)
                .title("Network Menu")
                .default_width(700)
                .default_height(600)
                .css_classes(["network-menu-window"])
                .build();

            // Setup CSS
            setup_css();

            // Create main layout
            let main_box = GtkBox::new(Orientation::Vertical, 0);

            // Add search entry
            let search_entry = SearchEntry::new();
            search_entry.set_margin_top(15);
            search_entry.set_margin_bottom(15);
            search_entry.set_margin_start(15);
            search_entry.set_margin_end(15);
            search_entry.set_hexpand(true);
            search_entry.set_placeholder_text(Some("Type to search..."));

            // Action list with scrolling
            let scrolled = ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .hscrollbar_policy(gtk4::PolicyType::Never) // Hide horizontal scrollbar
                .build();

            let action_list = ListBox::new();
            action_list.set_selection_mode(SelectionMode::Single);
            action_list.set_activate_on_single_click(true);

            // Populate list initially with all items
            let mut indices = indices_clone.lock().unwrap();
            update_action_list(&action_list, &actions_clone, "", &mut indices);
            drop(indices); // Release lock

            // Connect action selection
            let actions_ref = actions_clone.clone();
            let selected_ref = selected_clone.clone();
            let app_ref = app.clone();
            let indices_ref = indices_clone.clone();

            action_list.connect_row_activated(clone!(@strong actions_ref, @strong selected_ref, @strong app_ref, @strong indices_ref => move |_list, row| {
                let index = row.index();
                if index >= 0 {
                    let indices = indices_ref.lock().unwrap();
                    if let Some(&original_idx) = indices.get(index as usize) {
                        if let Some(action) = actions_ref.get(original_idx) {
                            *selected_ref.lock().unwrap() = Some(action.clone());
                            app_ref.quit();
                        }
                    }
                }
            }));

            // Setup filtering that updates immediately as user types
            let actions_ref = actions_clone.clone();
            let action_list_ref = action_list.clone();
            let indices_ref = indices_clone.clone();

            search_entry.connect_search_changed(clone!(@strong actions_ref, @strong action_list_ref, @strong indices_ref => move |entry| {
                let text = entry.text().to_string();
                let mut indices = indices_ref.lock().unwrap();
                update_action_list(&action_list_ref, &actions_ref, &text, &mut indices);
            }));

            // Add keyboard navigation
            let controller = EventControllerKey::new();
            let action_list_ref = action_list.clone();
            let search_entry_ref = search_entry.clone();
            let actions_ref = actions_clone.clone();
            let selected_ref = selected_clone.clone();
            let app_ref = app.clone();
            let indices_ref = indices_clone.clone();

            controller.connect_key_pressed(
                clone!(@strong action_list_ref, @strong search_entry_ref, @strong actions_ref, @strong selected_ref, @strong app_ref, @strong indices_ref => move |_, key, _, modifier| {
                    match key {
                        // Enter key - activate selected row
                        Key::Return => {
                            if let Some(row) = action_list_ref.selected_row() {
                                let index = row.index();
                                if index >= 0 {
                                    let indices = indices_ref.lock().unwrap();
                                    if let Some(&original_idx) = indices.get(index as usize) {
                                        if let Some(action) = actions_ref.get(original_idx) {
                                            *selected_ref.lock().unwrap() = Some(action.clone());
                                            app_ref.quit();
                                        }
                                    }
                                }
                            }
                            glib::Propagation::Stop
                        },
                        // Escape key - quit without error
                        Key::Escape => {
                            // Just quit the app, the outer code will return Ok(None)
                            app_ref.quit();
                            glib::Propagation::Stop
                        },
                        // Down arrow - move selection down
                        Key::Down => {
                            let current_idx = match action_list_ref.selected_row() {
                                Some(row) => row.index(),
                                None => -1,
                            };
                            let next_idx = current_idx + 1;
                            if let Some(next_row) = action_list_ref.row_at_index(next_idx) {
                                action_list_ref.select_row(Some(&next_row));
                                next_row.grab_focus();
                                glib::Propagation::Stop
                            } else {
                                glib::Propagation::Proceed
                            }
                        },
                        // Up arrow - move selection up
                        Key::Up => {
                            let current_idx = match action_list_ref.selected_row() {
                                Some(row) => row.index(),
                                None => 0,
                            };
                            if current_idx > 0 {
                                if let Some(prev_row) = action_list_ref.row_at_index(current_idx - 1) {
                                    action_list_ref.select_row(Some(&prev_row));
                                    prev_row.grab_focus();
                                    glib::Propagation::Stop
                                } else {
                                    glib::Propagation::Proceed
                                }
                            } else {
                                glib::Propagation::Proceed
                            }
                        },
                        // Tab key - cycle through items
                        Key::Tab => {
                            let total_rows = action_list_ref.observe_children().n_items();
                            if total_rows > 0 {
                                let current_idx = match action_list_ref.selected_row() {
                                    Some(row) => row.index(),
                                    None => -1,
                                };
                                let next_idx = if modifier.contains(ModifierType::SHIFT_MASK) {
                                    // Shift+Tab goes backward
                                    if current_idx <= 0 { total_rows as i32 - 1 } else { current_idx - 1 }
                                } else {
                                    // Tab goes forward
                                    (current_idx + 1) % total_rows as i32
                                };
                                if let Some(next_row) = action_list_ref.row_at_index(next_idx) {
                                    action_list_ref.select_row(Some(&next_row));
                                    next_row.grab_focus();
                                    glib::Propagation::Stop
                                } else {
                                    glib::Propagation::Proceed
                                }
                            } else {
                                glib::Propagation::Proceed
                            }
                        },
                        // Ctrl+J (Down) and Ctrl+K (Up) for vim-like navigation
                        _ if modifier.contains(ModifierType::CONTROL_MASK) => {
                            match key {
                                Key::j => { // Ctrl+J - move down
                                    let current_idx = match action_list_ref.selected_row() {
                                        Some(row) => row.index(),
                                        None => -1,
                                    };
                                    let next_idx = current_idx + 1;
                                    if let Some(next_row) = action_list_ref.row_at_index(next_idx) {
                                        action_list_ref.select_row(Some(&next_row));
                                        next_row.grab_focus();
                                        glib::Propagation::Stop
                                    } else {
                                        glib::Propagation::Proceed
                                    }
                                },
                                Key::k => { // Ctrl+K - move up
                                    let current_idx = match action_list_ref.selected_row() {
                                        Some(row) => row.index(),
                                        None => 0,
                                    };
                                    if current_idx > 0 {
                                        if let Some(prev_row) = action_list_ref.row_at_index(current_idx - 1) {
                                            action_list_ref.select_row(Some(&prev_row));
                                            prev_row.grab_focus();
                                            glib::Propagation::Stop
                                        } else {
                                            glib::Propagation::Proceed
                                        }
                                    } else {
                                        glib::Propagation::Proceed
                                    }
                                },
                                _ => glib::Propagation::Proceed,
                            }
                        },
                        _ => glib::Propagation::Proceed,
                    }
                })
            );

            // Ensure that all keys first go to the search entry
            search_entry.add_controller(controller);

            // Assemble UI
            scrolled.set_child(Some(&action_list));
            main_box.append(&search_entry);
            main_box.append(&scrolled);
            window.set_child(Some(&main_box));

            // Focus the search entry at start
            search_entry.grab_focus();

            // Show window
            window.present();
        }));

        // Run application
        let args: Vec<String> = Vec::new();
        let _ = app.run_with_args(&args);

        // After the application has run, check for the selected result
        if let Some(result) = selected.lock().unwrap().clone() {
            return Ok(Some(result));
        }

        // If GTK UI didn't return a selection (including Escape key press), just return None
        return Ok(None);
    }
}

// Helper function to use dmenu as fallback
fn use_dmenu_fallback(actions: &[String]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    eprintln!("Using dmenu fallback");

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
