//! Thinking spinner for the interactive shell

use indicatif::{ProgressBar, ProgressStyle};

/// A spinner shown while waiting for the first token
pub struct ThinkingSpinner {
    bar: ProgressBar,
    active: bool,
}

impl ThinkingSpinner {
    pub fn new() -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&[
                    "\u{2800}", "\u{2801}", "\u{2803}", "\u{2807}", "\u{280f}",
                    "\u{281f}", "\u{283f}", "\u{287f}", "\u{28ff}", "\u{28fe}",
                    "\u{28fc}", "\u{28f8}", "\u{28f0}", "\u{28e0}", "\u{28c0}",
                    "\u{2880}", "\u{2800}",
                ]),
        );
        Self { bar, active: false }
    }

    /// Start the spinner with a message
    pub fn start(&mut self, msg: &str) {
        self.bar.set_message(msg.to_string());
        self.bar.enable_steady_tick(std::time::Duration::from_millis(80));
        self.active = true;
    }

    /// Stop and clear the spinner
    pub fn stop(&mut self) {
        if self.active {
            self.bar.finish_and_clear();
            self.active = false;
        }
    }

    /// Check if the spinner is currently active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for ThinkingSpinner {
    fn drop(&mut self) {
        self.stop();
    }
}
