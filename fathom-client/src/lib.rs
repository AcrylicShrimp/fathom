mod runtime;
mod tui;
mod util;
mod view;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

pub use runtime::bootstrap_demo;
pub use tui::run_tui;
