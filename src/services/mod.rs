//! 系统服务: the right sidebar's tool that lists the systemd services and
//! timers of the host of the SSH terminal in front, starts, stops and
//! restarts the services, starts and stops the timers and runs what they
//! start now, and shows their state and journal, over that terminal's own
//! connection. Linux hosts run by systemd only, for now.

mod linux;
mod model;
mod service_details;
mod service_panel;
mod timer_details;

pub use linux::{control_command, controlled};
pub use model::{HostClock, Service, ServiceCommand, Timer};
pub use service_details::open_service_dialog;
pub use service_panel::ServicePanel;
pub use timer_details::open_timer_dialog;
