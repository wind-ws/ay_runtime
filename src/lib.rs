#![feature(context_ext)] // 允许Context附加扩展数据,进入Future::poll
#![feature(local_waker)]
// #![feature(downcast_unchecked)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::new_without_default)]

pub mod runtime;
pub mod tcp;
pub mod trying;
pub mod utils;
