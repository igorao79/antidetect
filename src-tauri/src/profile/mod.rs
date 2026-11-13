pub(crate) mod after_exit;
mod camoufox;
pub mod clear_on_close;
pub mod clone_copies;
pub mod encryption;
pub mod manager;
pub mod password;
pub mod patch;
pub mod portable;
pub mod quick;
pub mod trash;
pub mod types;

pub use manager::ProfileManager;
pub use types::BrowserProfile;
