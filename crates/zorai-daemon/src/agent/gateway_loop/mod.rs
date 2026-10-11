//! Gateway initialization, background run loop, and platform message polling.

use super::gateway_health::{GatewayConnectionStatus, PlatformHealthState};
use super::heartbeat::is_peak_activity_hour;
use super::*;
use chrono::Timelike;
use std::sync::OnceLock;

mod lifecycle;
mod message_flow;
mod message_helpers;
mod replay;
mod run_loop;
mod state;
mod supervision;
mod thread_links;

#[allow(unused_imports)]
pub(super) use lifecycle::*;
#[allow(unused_imports)]
pub(super) use message_flow::*;
#[allow(unused_imports)]
pub(super) use message_helpers::*;
#[allow(unused_imports)]
pub(super) use replay::*;
#[allow(unused_imports)]
pub(super) use run_loop::*;
#[allow(unused_imports)]
pub(super) use state::*;
#[allow(unused_imports)]
pub(super) use supervision::*;

#[cfg(test)]
mod tests;
