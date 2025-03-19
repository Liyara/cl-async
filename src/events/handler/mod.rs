use crate::task::Task;

use super::{
    subscription_handle::SubscriptionHandle, 
    Event
};

pub mod registry;
pub use registry::EventHandlerRegistry;

pub trait EventHandler {
    fn handle(&self, event: Event, handle: SubscriptionHandle) -> Option<Task>;
}