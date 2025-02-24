use std::{
    pin::Pin,
    sync::{Arc, atomic::AtomicU64},
    task::Wake,
};

use crate::utils::pipe::Pipe;

pub type MyFuture<O = ()> = Pin<Box<dyn Future<Output = O> + Send>>;
pub type ID = u64;

pub struct Task<O = ()> {
    pub id: ID,
    pub future: MyFuture<O>,
}
impl Task {
    pub fn waker(&self, pipe_write: Arc<Pipe<ID>>) -> TaskWaker {
        TaskWaker {
            id: self.id,
            pipe_write,
        }
    }
}
pub struct TaskWaker {
    pub id: ID,
    pub pipe_write: Arc<Pipe<ID>>,
}
impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.pipe_write.write(&self.id);
    }
}

pub fn get_id() -> ID {
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
