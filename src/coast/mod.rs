//! 海岸线与陆地填充。

mod land;
mod line;
mod view;

pub use land::{setup_ldfill, update_ldfill, Ldfill};
pub use line::{setup_coastl, Coastl};
pub use view::{setup_coastv, update_coastv, Coastv};