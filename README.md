
# lockless_async_runtime


用epoll（只支持linux）实现一个最小功能的**线程池**异步运行时，具体效果就是
1、实现一个TcpStream，具体api仿照std里的TcpStream，但是是异步版本
2、实现一个在main上的attribute proc macro，类似于tokio::main和tokio::test，使得可以将main或单元测试变成异步函数，并且不破坏rust analyzer对于函数内容的类型标注、鼠标悬停提示等。

要求，不使用任何形式的锁，包含std给的锁、parking_lot的锁或变相的自旋锁逻辑。

最新版rustc，可使用所有的非incomplete的unstable功能。
可使用std和所有跟线程同步、异步基础设施无关的crate。
代码中可有unsafe，但不可存在ub。
需要以下检查通过，并且没有任何错误/警告：
cargo miri test
cargo clippy
cargo fmt --check


# Note
* 不可使用锁,不可存在ub,不可使用 关于 线程同步,异步基础设施 的crate
* 不可包含 任何 锁逻辑(库中也不可包含,除了epoll)
* 不可存在 任何 堵塞(库中也不可包含,除了epoll)

# Bugs

## 仅只有在 使用 MyTcpStream 时,会发生以下错误
signal SIGABRT  
malloc(): unaligned tcache chunk detected

奇怪的时,每次发生错误的地点还不同,有时还不会发生错误
几乎都发生在 `let c = Arc::new(b);` 这个代码中的Box::new中
但可能实际错误不是发生在这里,只是这里触发了abrot

只有调用 Socket连接才会出现


调用
Arc::new(TaskWaker {
    id: task.id,
    pipe_write:Arc::new(pipe::Pipe::<ID>::new()),
});
会发生abrot,
但
Arc::new(Arc::new(pipe::Pipe::<ID>::new()));
却不会

已确定: 是pipe的问题
由于pipe write 不会保存所有权,而且只发送指针区域的内容,导致有些堆区的内容被释放,而使用了被释放的堆区

## 当task数量多起来后,就会发生崩溃
十有八九是 pipe的问题
被骗了,谁说pipe可以多线程多消费多生产的...

pipe没有出现 重复读取重复写入的问题
可能是和pipe相关,

bug 展现出随机性,每次运行结果都可能不同,有以下几种:
1. malloc(): unaligned tcache chunk detected (signal: 11, SIGSEGV: invalid memory reference)
2. malloc_consolidate(): unaligned fastbin chunk detected (signal: 6, SIGABRT: process abort signal)
3. malloc(): unaligned tcache chunk detected (signal: 6, SIGABRT: process abort signal)
4. corrupted double-linked list
5. 运行成功