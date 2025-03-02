use std::time::Instant;

use lazy_static::lazy_static;

pub mod epoll;
pub mod eventfd;
pub mod mman;
pub mod pipe;
pub mod socket;
pub mod timefd;
pub mod mq;

lazy_static! {
    pub static ref NOW: Instant = Instant::now();
}
