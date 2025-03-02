use std::{os::fd::RawFd, ptr::NonNull, sync::atomic::AtomicU64};

pub type ID = u64;

pub fn get_id() -> ID {
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct Task<O = ()> {
    pub id: ID,
    pub future: NonNull<(dyn Future<Output = O> + Send)>,
}
impl Task {
    pub fn new(id: ID, future: Box<dyn Future<Output = ()> + Send>) -> Self {
        let ptr = Box::into_raw(future);
        Self {
            id,
            future: NonNull::new(ptr).unwrap(),
        }
    }
    pub fn drop(self) {
        drop(unsafe { Box::from_raw(self.future.as_ptr()) })
    }
}

pub struct ExtData {
    pub epoll_fd: RawFd,
    pub id: ID,
}
