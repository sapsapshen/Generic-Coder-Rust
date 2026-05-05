//! Event handling types and helpers.

use crossterm::event::{Event, poll, read};
use std::time::Duration;

/// Which panel currently has focus
#[derive(Clone, Copy, PartialEq)]
pub enum Panel {
    Chat,
    Sidebar,
}

/// Input mode determines how keystrokes are interpreted
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum InputMode {
    Normal,   // navigation and commands
    Insert,   // typing into chat input
    Settings, // editing settings fields
}

/// Read a single event with a short timeout for polling
pub fn read_event() -> std::io::Result<Event> {
    const POLL_MS: u64 = 50;
    loop {
        if poll(Duration::from_millis(POLL_MS))? {
            return read();
        }
    }
}
