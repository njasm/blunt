pub mod task {
    use std::future::Future;
    use tokio::task::JoinHandle;

    pub fn spawn<T>(future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        tokio::task::spawn(future)
    }
}

pub mod mpsc {
    use tokio::sync::mpsc::unbounded_channel as tokio_unbounded_channel;

    pub use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

    pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        tokio_unbounded_channel::<T>()
    }

    pub mod error {
        pub use tokio::sync::mpsc::error::SendError;
    }
}
