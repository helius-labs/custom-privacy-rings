//! One processor per instruction tag of the Storage program.

mod apply_enrollments;
mod initialize;
mod initialize_tracking;
mod owner_grant;
mod update;
mod update_authority;

pub use apply_enrollments::process_apply_enrollments_ix;
pub use initialize::process_initialize_ix;
pub use initialize_tracking::process_initialize_tracking_ix;
pub use owner_grant::process_owner_grant_ix;
pub use update::process_update_ix;
pub use update_authority::process_update_authority_ix;
