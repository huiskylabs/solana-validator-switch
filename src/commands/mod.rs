pub mod config;
pub mod setup;
pub mod status;
pub mod switch;

pub use config::config_command;
pub use setup::setup_command;
pub use status::status_command;
pub use switch::switch_command;