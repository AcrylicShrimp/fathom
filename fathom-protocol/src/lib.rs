mod labels;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

pub use labels::{
    execution_status_label, execution_update_phase_label, refresh_scope_label,
    system_notice_level_label,
};
