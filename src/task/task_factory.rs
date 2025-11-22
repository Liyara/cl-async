use std::pin::Pin;


pub trait TaskFactory: Send + 'static {

    type Fut: Future<Output = ()> + 'static;

    fn create_task(self) -> Self::Fut;
}

impl<F, Fut> TaskFactory for F 
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static
{
    type Fut = Fut;

    fn create_task(self) -> Self::Fut {
        (self)()
    }
}

pub trait ErasedTaskFactory: Send + 'static {
    fn create_task_boxed(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
}

impl<T> ErasedTaskFactory for T
where
    T: TaskFactory,
{
    fn create_task_boxed(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
        Box::pin(TaskFactory::create_task(*self))
    }
}

pub struct BoxTaskFactory {
    factory: Box<dyn ErasedTaskFactory>
}

impl BoxTaskFactory {
    pub fn create_task_boxed(self) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
        self.factory.create_task_boxed()
    }
}

pub fn box_task_factory<T>(
    factory: T
) -> BoxTaskFactory 
where
    T: TaskFactory,
{
    BoxTaskFactory { factory: Box::new(factory) }
}