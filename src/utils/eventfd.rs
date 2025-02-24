pub fn eventfd(init:u32, flags:i32) {
    unsafe { libc::eventfd(init, flags); }
    
}
pub fn eventfd_read() {
    // libc::eventfd_read(fd, value)
}
pub fn eventfd_write() {
    // libc::eventfd_write(fd, value)
}
