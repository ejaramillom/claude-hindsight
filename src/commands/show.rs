//! Implementation of the `show` command
//!
//! Displays execution tree for a Claude Code session.

use crate::error::Result;
use crate::tui::router::Router;
use crate::tui::run_router;

pub fn run(session_id: String, dashboard: bool, _port: u16) -> Result<()> {
    if dashboard {
        println!("  Web dashboard not yet implemented");
        println!("  Use without --dashboard flag to view in terminal\n");
        return Ok(());
    }

    // Launch TUI with specific session
    let router = Router::new_with_session(session_id)?;
    run_router(router)
}
