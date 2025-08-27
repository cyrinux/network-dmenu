#[cfg(feature = "gtk-ui")]
use gtk4::{
    gdk::{Display, Key, ModifierType},
    glib::{self, clone},
    pango,
    prelude::*,
    Application, ApplicationWindow, Box as GtkBox, CssProvider, EventControllerKey, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SearchEntry, SelectionMode,
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(feature = "gtk-ui")]
use std::sync::{Arc, Mutex};

// Helper functions for emoji detection will be handled directly in the update_action_list function

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

        let row_box = GtkBox::new(Orientation::Horizontal, 0);
        row_box.set_spacing(8);

        // Extract category prefix if present
        let (category, rest) = if let Some(dash_pos) = action.find(" - ") {
            let (cat, r) = action.split_at(dash_pos + 3); // +3 to include " - "
            (cat.trim(), r)
        } else {
            ("", action.as_str())
        };

        // Create category label if category exists
        if !category.is_empty() {
            let category_label = Label::new(Some(&format!("{:<10} -", category)));
            category_label.set_xalign(0.0);
            category_label.set_width_chars(10);
            category_label.set_max_width_chars(10);
            category_label.add_css_class("category-label");
            row_box.append(&category_label);
        }

        // Now we need to handle the content part (icon + text)
        // First, determine if the content starts with an emoji or icon
        let mut content_chars = rest.chars();
        let has_icon = content_chars.next().map_or(false, |c| {
            c == '🔴' || c == '🟢' || c == '🔵' || c == '🟠' ||
            c == '✅' || c == '❌' || c == '📶' || c == '🔒' ||
            c == '🔓' || c == '🌐' || c == '📡' || c == '🔍' ||
            c == '🛡' || c == '⚠' || c == '🧩' ||
            (c >= '\u{1F1E6}' && c <= '\u{1F1FF}') // Regional indicator symbols (flag emojis)
        });

        // If we have an icon/emoji, split it off from the rest
        let (icon, main_text) = if has_icon {
            // For flags (which are two regional indicators), take both characters
            let is_flag = rest.len() >= 2 &&
                           rest.chars().next().map_or(false, |c| c >= '\u{1F1E6}' && c <= '\u{1F1FF}') &&
                           rest.chars().nth(1).map_or(false, |c| c >= '\u{1F1E6}' && c <= '\u{1F1FF}');

            if is_flag {
                // Take exactly the flag emoji (2 chars)
                let mut chars_iter = rest.char_indices();
                if let (Some(_), Some((idx, _))) = (chars_iter.next(), chars_iter.next()) {
                    if let Some((end_idx, _)) = chars_iter.next() {
                        (&rest[..end_idx], rest[end_idx..].trim_start())
                    } else {
                        (rest, "") // The whole string is just the flag
                    }
                } else {
                    ("", rest) // Shouldn't happen, but just in case
                }
            } else {
                // For other emojis/icons, take the first word
                match rest.find(' ') {
                    Some(space_idx) => (&rest[..space_idx], rest[space_idx..].trim_start()),
                    None => (rest, "") // No space found, the whole thing is the icon
                }
            }
        } else {
            ("", rest) // No icon/emoji
        };

        // Add the icon label if we have an icon
        if !icon.is_empty() {
            let icon_label = Label::new(Some(icon));
            icon_label.set_width_chars(2);
            icon_label.set_xalign(0.0);
            icon_label.add_css_class("icon-label");
            row_box.append(&icon_label);
        } else {
            // Add an empty space for alignment
            let spacer = Label::new(Some(" "));
            spacer.set_width_chars(2);
            row_box.append(&spacer);
        }

        // Create and add the main text label
        let text_label = Label::new(None);
        text_label.set_xalign(0.0);
        text_label.set_hexpand(true);
        text_label.set_margin_start(0);
        text_label.set_margin_end(0);
        text_label.set_ellipsize(pango::EllipsizeMode::End);
        text_label.set_max_width_chars(80);

        // Apply highlighting if needed
        if let Some(positions) = match_positions {
            let mut markup = String::with_capacity(main_text.len() * 2);

            // Calculate the prefix length for position adjustment
            let prefix_len = if category.is_empty() { 0 } else { category.len() + 3 };

            // Calculate icon length for position adjustment
            let icon_len = if !icon.is_empty() {
                // Count the icon chars plus the space after it
                icon.chars().count() + 1
            } else {
                0
            };

            // Apply highlighting to main text
            for (i, c) in main_text.chars().enumerate() {
                let orig_pos = i + prefix_len + icon_len;
                if positions.contains(&orig_pos) {
                    // Highlight matched character
                    markup.push_str(&format!("<span foreground=\"#fcaf3e\" weight=\"bold\">{}</span>",
                                          glib::markup_escape_text(&c.to_string())));
                } else {
                    // Regular character
                    markup.push_str(&glib::markup_escape_text(&c.to_string()));
                }
            }

            text_label.set_markup(&markup);
        } else {
            // No highlighting needed
            text_label.set_text(main_text);
        }

        row_box.append(&text_label);
        row.set_child(Some(&row_box));
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
            background-color: #222222;
        }
        label {
            color: #d3d7cf;
            font-family: 'monospace';
            font-size: 10pt;
            padding: 0px;
        }
        tooltip {
            background-color: #2a2a2a;
            color: #d3d7cf;
        }
        .category-label {
            color: #888a85;
            margin-left: 2px;
            min-width: 80px;
            margin-right: 4px;
        }
        .icon-label {
            margin-right: 6px;
            margin-left: 2px;
            min-width: 25px;
        }
        entry {
            color: #d3d7cf;
            background-color: #2f2f2f;
            padding: 4px 8px;
            font-size: 11pt;
            font-family: 'monospace';
            border: none;
            caret-color: #fcaf3e;
            margin: 0px;
        }
        entry selection {
            background-color: #215d9c;
            color: #ffffff;
        }
        entry:focus {
            border: none;
            outline: none;
        }
        listbox {
            background-color: #222222;
        }
        listboxrow {
            padding: 0px;
            margin: 0px;
        }
        listboxrow:selected {
            background-color: #215d9c;
        }
        listboxrow:hover {
            background-color: #303030;
        }
        scrolledwindow {
            border: none;
            background-color: #222222;
        }
        .counter-label {
            color: #888a85;
            font-size: 9pt;
        }
        .statusbar {
            background-color: #242424;
            color: #888a85;
            font-size: 9pt;
            padding: 4px 6px;
            border-top: 1px solid #3a3a3a;
        }
        .shortcut-key {
            color: #fcaf3e;
            font-weight: bold;
        }
        .shortcuts-box {
            padding: 0px 2px;
        }
        .shortcuts-box label {
            margin: 0px 2px;
        }
        .shortcuts-tooltip {
            background-color: #333333;
            padding: 10px;
            border-radius: 5px;
            border: 1px solid #444444;
        }
        .shortcut-key {
            color: #fcaf3e;
            font-weight: bold;
        }
        .shortcuts-title {
            font-weight: bold;
            margin-bottom: 8px;
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

        // Create an Arc for the matched count
        let matched_count = Arc::new(Mutex::new(actions_clone.len()));
        let matched_count_clone = matched_count.clone();

        // Create an Arc for the label
        let position_label = Arc::new(Mutex::new(None::<Label>));
        let position_label_clone = position_label.clone();

        // Define keyboard shortcuts for different sections
        let shortcuts = [
            ("Alt+t", "tailscale"),
            ("Alt+w", "wifi"),
            ("Alt+m", "mullvad"),
            ("Alt+b", "bluetooth"),
            ("Alt+s", "sign"),
            ("Alt+e", "exit-node"),
            ("Alt+v", "vpn"),
            ("Alt+d", "diagnostic"),
        ];

        // (Removed unused help text)

        app.connect_activate(clone!(@strong actions_clone, @strong selected_clone, @strong indices_clone => move |app| {
            // Setup window
            let window = ApplicationWindow::builder()
                .application(app)
                .title("Network Menu")
                .default_width(1000)
                .default_height(800)
                .css_classes(["network-menu-window"])
                .icon_name("network-wireless")
                .resizable(true)
                .build();

            // Create a clone for use in closures
            let window_clone = window.clone();

            // Create a tooltip string with all shortcuts
            let mut shortcuts_text = String::from("Keyboard Shortcuts:\n\n");

            // Add filter shortcuts
            shortcuts_text.push_str("Filter Shortcuts:\n");
            for (key, target) in shortcuts.iter() {
                shortcuts_text.push_str(&format!("{}: Search for '{}'\n", key, target));
            }

            // Add navigation shortcuts
            shortcuts_text.push_str("\nNavigation:\n");
            shortcuts_text.push_str("↑/↓: Navigate through items\n");
            shortcuts_text.push_str("Enter: Select current item\n");
            shortcuts_text.push_str("Escape: Cancel selection\n");
            shortcuts_text.push_str("F1: Show/hide this help");

            // Set tooltip on main window
            window.set_tooltip_markup(Some(&shortcuts_text));
            window.set_has_tooltip(true);

            // Setup CSS
            setup_css();

            // Create main layout
            let main_box = GtkBox::new(Orientation::Vertical, 0);

            // Add search entry
            let search_entry = SearchEntry::new();
            search_entry.set_margin_top(4);
            search_entry.set_margin_bottom(4);
            search_entry.set_margin_start(2);
            search_entry.set_margin_end(2);
            search_entry.set_hexpand(true);
            search_entry.set_placeholder_text(Some("Type to search... (Alt+key for shortcuts)"));
            search_entry.set_width_chars(30);
            search_entry.set_max_width_chars(50);
            // search_entry.set_has_frame(false); // Not available for SearchEntry

            // Add a counter label next to the search entry (showing something like "553/553")
            let counter_label = Label::new(Some(&format!("{}/{}", actions_clone.len(), actions_clone.len())));
            counter_label.set_margin_top(4);
            counter_label.set_margin_end(6);
            counter_label.add_css_class("counter-label");

            // Show current position in list (e.g., "553/553")
            let pos_label = Label::new(Some(""));
            pos_label.set_margin_start(8);
            pos_label.add_css_class("counter-label");

            // Store position label for later access
            {
                let mut label_ref = position_label.lock().unwrap();
                *label_ref = Some(pos_label.clone());
            }

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

            search_entry.connect_search_changed(clone!(@strong actions_ref, @strong action_list_ref, @strong indices_ref, @strong matched_count_clone, @strong counter_label, @strong position_label => move |entry| {
                let text = entry.text().to_string();
                let mut indices = indices_ref.lock().unwrap();
                update_action_list(&action_list_ref, &actions_ref, &text, &mut indices);

                // Update counter
                let mut count = matched_count_clone.lock().unwrap();
                *count = indices.len();
                counter_label.set_text(&format!("{}/{}", indices.len(), actions_ref.len()));

                // Reset position indicator when search changes
                if let Some(label) = position_label.lock().unwrap().as_ref() {
                    label.set_text("");
                }
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
                clone!(@strong action_list_ref, @strong search_entry_ref, @strong actions_ref, @strong selected_ref, @strong app_ref, @strong indices_ref, @strong window_clone, @strong position_label_clone => move |_, key, _, modifier| {
                    // Handle Alt+key shortcut bindings
                    if modifier.contains(ModifierType::ALT_MASK) {
                        match key {
                            Key::t => {
                                search_entry_ref.grab_focus();
                                search_entry_ref.set_text("tailscale");
                                search_entry_ref.set_position(-1); // Move cursor to end
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::w => {
                                search_entry_ref.set_text("wifi");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::m => {
                                search_entry_ref.set_text("mullvad");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::b => {
                                search_entry_ref.set_text("bluetooth");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::s => {
                                search_entry_ref.set_text("sign");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::e => {
                                search_entry_ref.set_text("exit-node");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::v => {
                                search_entry_ref.set_text("vpn");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            Key::d => {
                                search_entry_ref.set_text("diagnostic");
                                search_entry_ref.set_position(-1);
                                // Manually trigger the search changed event by setting text again
                                let current_text = search_entry_ref.text().to_string();
                                search_entry_ref.set_text(&current_text);
                                return glib::Propagation::Stop;
                            },
                            _ => {}
                        }
                    }

                    // F1 key - toggle tooltips visibility
                    if key == Key::F1 {
                        window_clone.trigger_tooltip_query();
                        return glib::Propagation::Stop;
                    }

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

                                // Update position indicator
                                let new_text = format!("{}↓", next_idx + 1);
                                if let Some(label) = position_label_clone.lock().unwrap().as_ref() {
                                    label.set_text(&new_text);
                                }

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

                                    // Update position indicator
                                    let new_text = format!("{}↑", current_idx);
                                    if let Some(label) = position_label_clone.lock().unwrap().as_ref() {
                                        label.set_text(&new_text);
                                    }

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

                                    // Update position indicator
                                    let new_text = format!("{}↻", next_idx + 1);
                                    if let Some(label) = position_label_clone.lock().unwrap().as_ref() {
                                        label.set_text(&new_text);
                                    }

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

                                        // Update position indicator
                                        let new_text = format!("{}↓", next_idx + 1);
                                        if let Some(label) = position_label_clone.lock().unwrap().as_ref() {
                                            label.set_text(&new_text);
                                        }

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

                                            // Update position indicator
                                            let new_text = format!("{}↑", current_idx);
                                            if let Some(label) = position_label_clone.lock().unwrap().as_ref() {
                                                label.set_text(&new_text);
                                            }

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

            // Create a search box with counter and position
            let search_box = GtkBox::new(Orientation::Horizontal, 0);

            // Add a small prefix label to mimic dmenu style
            let prefix_label = Label::new(Some("|"));
            prefix_label.set_margin_start(5);
            prefix_label.set_margin_end(5);
            prefix_label.add_css_class("counter-label");

            search_box.append(&prefix_label);
            search_box.append(&search_entry);
            search_box.append(&counter_label);
            search_box.append(&pos_label);

            // Create a status bar with shortcut keys
            let status_bar = GtkBox::new(Orientation::Horizontal, 4);
            status_bar.add_css_class("statusbar");
            status_bar.set_margin_top(2);

            // Add help hint for F1
            let help_label = Label::new(None);
            help_label.set_markup("<span foreground=\"#fcaf3e\" weight=\"bold\">F1</span>: Help");
            help_label.set_margin_end(15);
            status_bar.append(&help_label);

            // Add navigation hints
            // Navigation section
            let nav_box = GtkBox::new(Orientation::Horizontal, 2);
            nav_box.add_css_class("shortcuts-box");

            let nav_label = Label::new(Some("Navigation:"));
            nav_label.set_margin_end(4);
            nav_box.append(&nav_label);

            let arrows_label = Label::new(None);
            arrows_label.set_markup("<span foreground=\"#fcaf3e\" weight=\"bold\">↑↓</span>: Navigate");
            arrows_label.set_margin_end(4);
            nav_box.append(&arrows_label);

            let enter_label = Label::new(None);
            enter_label.set_markup("<span foreground=\"#fcaf3e\" weight=\"bold\">Enter</span>: Select");
            enter_label.set_margin_end(4);
            nav_box.append(&enter_label);

            let esc_label = Label::new(None);
            esc_label.set_markup("<span foreground=\"#fcaf3e\" weight=\"bold\">Esc</span>: Cancel");
            esc_label.set_margin_end(8);
            nav_box.append(&esc_label);

            status_bar.append(&nav_box);

            // Separator
            let separator = Label::new(Some("|"));
            separator.set_margin_start(2);
            separator.set_margin_end(8);
            status_bar.append(&separator);

            // Filters section
            let filters_box = GtkBox::new(Orientation::Horizontal, 2);
            filters_box.add_css_class("shortcuts-box");

            let filters_label = Label::new(Some("Filters:"));
            filters_label.set_margin_end(4);
            filters_box.append(&filters_label);

            // Add shortcut keys in a scrollable horizontal box
            let shortcuts_box = GtkBox::new(Orientation::Horizontal, 4);
            shortcuts_box.set_hexpand(true);

            for (i, (key, target)) in shortcuts.iter().enumerate() {
                let shortcut_label = Label::new(None);
                shortcut_label.set_markup(&format!("<span foreground=\"#fcaf3e\" weight=\"bold\">{}</span>: {}", key, target));
                shortcut_label.set_margin_end(6);
                shortcuts_box.append(&shortcut_label);

                // Add separator after each shortcut except the last
                if i < shortcuts.len() - 1 {
                    let separator = Label::new(Some("|"));
                    separator.set_margin_start(1);
                    separator.set_margin_end(1);
                    shortcuts_box.append(&separator);
                }
            }

            filters_box.append(&shortcuts_box);
            status_bar.append(&filters_box);

            // Assemble UI
            scrolled.set_child(Some(&action_list));
            main_box.append(&search_box);
            main_box.append(&scrolled);
            main_box.append(&status_bar);
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
