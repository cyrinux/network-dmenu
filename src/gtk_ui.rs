use crate::constants::{ICON_CHECK, ICON_LEAF};
use crate::tailscale::TailscaleAction;
use gtk4::{
    gdk::Display,
    gio::{self, SimpleAction},
    glib::{self, clone},
    prelude::*,
    Application, ApplicationWindow, Box, CssProvider, Entry, Label, ListBox, ListBoxRow,
    Orientation, ScrolledWindow, SearchEntry, StyleContext, Widget,
};
use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, RelmApp, RelmComponent, SimpleComponent,
};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

// Application ID for GTK
const APP_ID: &str = "org.cyrinux.network_dmenu";

// Define messages for our component
#[derive(Debug)]
pub enum Message {
    Filter(String),
    ActionSelected(usize),
    Close,
}

// Define the model that holds our component state
pub struct AppModel {
    actions: Vec<String>,                  // List of actions to display
    filtered_actions: Vec<String>,         // Filtered list based on search
    callback: Arc<Mutex<Option<usize>>>,   // Callback for selected action index
    tx: mpsc::Sender<Option<usize>>,       // Channel to send the selection
}

// Struct to hold UI widgets
#[derive(Debug)]
pub struct AppWidgets {
    window: ApplicationWindow,
    search_entry: SearchEntry,
    action_list: ListBox,
}

// Implementation of our component
impl SimpleComponent for AppModel {
    type Init = Vec<String>;
    type Input = Message;
    type Output = ();
    type Root = ApplicationWindow;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        let window = ApplicationWindow::builder()
            .title("Network Menu")
            .default_width(600)
            .default_height(400)
            .build();

        window
    }

    fn init(
        actions: Self::Init,
        window: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Create channels for returning selection
        let (tx, mut rx) = mpsc::channel(1);
        let callback = Arc::new(Mutex::new(None));

        let callback_clone = callback.clone();
        tokio::spawn(async move {
            if let Some(selection) = rx.recv().await {
                *callback_clone.lock().unwrap() = Some(selection);
            }
        });

        // Setup widgets
        let main_box = Box::new(Orientation::Vertical, 0);

        // Search entry at top
        let search_entry = SearchEntry::new();
        search_entry.set_margin_all(10);
        search_entry.set_hexpand(true);
        search_entry.grab_focus();

        // Action list with scrolling
        let scrolled = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .build();

        let action_list = ListBox::new();
        action_list.set_selection_mode(gtk4::SelectionMode::Single);
        action_list.set_activate_on_single_click(true);

        // Connect search entry to filter function
        search_entry.connect_search_changed(clone!(@strong sender => move |entry| {
            let text = entry.text().to_string();
            sender.input(Message::Filter(text));
        }));

        // Connect action list to selection
        action_list.connect_row_activated(clone!(@strong sender => move |_, row| {
            if let Some(index) = row.index() {
                sender.input(Message::ActionSelected(index as usize));
            }
        }));

        // Key event handler for Escape to close
        window.connect_key_press_event(clone!(@strong sender => move |_, key| {
            if key.keyval() == gdk::keys::constants::Escape {
                sender.input(Message::Close);
                Inhibit(true)
            } else {
                Inhibit(false)
            }
        }));

        // Assemble UI
        scrolled.set_child(Some(&action_list));
        main_box.append(&search_entry);
        main_box.append(&scrolled);
        window.set_child(Some(&main_box));

        // Initial population of the list
        let filtered_actions = actions.clone();
        update_action_list(&action_list, &filtered_actions);

        // Create model and widgets
        let model = AppModel {
            actions,
            filtered_actions,
            callback,
            tx,
        };

        let widgets = AppWidgets {
            window: window.clone(),
            search_entry,
            action_list,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, widgets: &mut Self::Widgets) {
        match message {
            Message::Filter(text) => {
                // Filter actions based on search text
                if text.is_empty() {
                    self.filtered_actions = self.actions.clone();
                } else {
                    let lowercase_text = text.to_lowercase();
                    self.filtered_actions = self.actions
                        .iter()
                        .filter(|action| action.to_lowercase().contains(&lowercase_text))
                        .cloned()
                        .collect();
                }
                update_action_list(&widgets.action_list, &self.filtered_actions);

                // Select first item if there are results
                if !self.filtered_actions.is_empty() {
                    widgets.action_list.select_row(widgets.action_list.row_at_index(0).as_ref());
                }
            },
            Message::ActionSelected(index) => {
                // Send selected action index
                if index < self.filtered_actions.len() {
                    // Find the original index in the unfiltered list
                    if let Some(action) = self.filtered_actions.get(index) {
                        if let Some(original_index) = self.actions.iter().position(|a| a == action) {
                            let _ = self.tx.try_send(Some(original_index));
                            widgets.window.close();
                        }
                    }
                }
            },
            Message::Close => {
                // User canceled
                let _ = self.tx.try_send(None);
                widgets.window.close();
            }
        }
    }
}

// Helper function to update the action list
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
        label.set_margin_all(10);
        row.set_child(Some(&label));
        list_box.append(&row);
    }
}

// Setup CSS styling
fn setup_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        b"
        window { background-color: #292a2e; }
        label { color: #ffffff; font-family: 'monospace'; }
        entry { color: #ffffff; background-color: #3a3b3f; border-radius: 5px; }
        listbox { background-color: #292a2e; }
        listboxrow:selected { background-color: #4a4b4f; }
        "
    );

    StyleContext::add_provider_for_display(
        &Display::default().expect("Could not get default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

// The main function to show the GTK UI and get user selection
pub async fn show_gtk_menu(actions: Vec<String>) -> Option<usize> {
    // Create a callback for getting the result
    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    // Run the GTK app in a separate thread
    std::thread::spawn(move || {
        let app = Application::builder()
            .application_id(APP_ID)
            .build();

        app.connect_activate(move |app| {
            setup_css();

            let window = ApplicationWindow::builder()
                .application(app)
                .title("Network Menu")
                .default_width(600)
                .default_height(400)
                .build();

            let model = AppModel::init(
                actions.clone(),
                &window,
                ComponentSender::default(),
            );

            window.present();
        });

        app.run();

        // After app exits, get the result
        *result_clone.lock().unwrap()
    });

    // Poll for result
    let mut attempts = 0;
    while attempts < 100 {
        if let Some(selection) = *result.lock().unwrap() {
            return Some(selection);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }

    None
}

// Function to be called from main.rs to use GTK UI instead of dmenu
pub async fn select_action_with_gtk(actions: Vec<String>) -> Option<String> {
    match show_gtk_menu(actions.clone()).await {
        Some(index) => actions.get(index).cloned(),
        None => None,
    }
}
