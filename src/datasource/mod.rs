pub mod process;
pub mod wezterm;

pub use process::{ProcessDataSource, ProcessInfo, ProcessTree, SystemProcessDataSource};
pub use wezterm::WeztermDataSource;

use crate::models::Pane;
use anyhow::Result;

/// Pane data source trait
pub trait PaneDataSource {
    fn list_panes(&self) -> Result<Vec<Pane>>;
}
