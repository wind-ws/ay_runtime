

// Executor 负责分配 Future 给 A线程 执行

// Executor 如何分配任务:
// 只管将Task 推入 pipe,
// 由A线程去争抢任务,

// id 全局唯一(不回收)

// Reactor 负责捕捉epoll事件 和 调用对应waker
// 

// waker 负责 告诉A线程再次执行 对应Future

// A线程 负责执行和管理Future

// Future::poll 负责 将waker进注册 reactor 由epoll事件触发执行waker
//
// 调用Future::poll 时的 Context 附带 context_ext(扩展数据)(包含: id,reactor_writer )
// 若 poll Ready,则 消耗 对应 id 的 Future,
// 若 poll Pending,则 通过 epoll 注册 需要的 interest_fd 且 附带id,

// 无论如何,都需要 线程信息传输,方案有一下几种:
// 2. 使用 pipe + epoll 通信
