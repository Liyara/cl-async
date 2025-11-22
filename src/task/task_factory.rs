use std::pin::Pin;


pub trait TaskSpawner: Send + 'static {

    type Fut: Future<Output = ()> + 'static;

    fn spawn_task(self) -> Self::Fut;
}


impl<F, Fut> TaskSpawner for F 
where
    F: (FnOnce() -> Fut) + Send + 'static,
    Fut: Future<Output = ()> + 'static
{
    type Fut = Fut;

    fn spawn_task(self) -> Self::Fut {
        (self)()
    }
}

pub trait BoxedTaskSpawner: Send + 'static {
    fn spawn_boxed_task(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
}

impl<T> BoxedTaskSpawner for T
where
    T: TaskSpawner,
{
    fn spawn_boxed_task(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
        Box::pin(TaskSpawner::spawn_task(*self))
    }
}

pub struct BoxTaskSpawner {
    factory: Box<dyn BoxedTaskSpawner>
}

impl BoxTaskSpawner {
    pub fn create_task_boxed(self) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
        self.factory.spawn_boxed_task()
    }
}

pub fn box_task_spawner<T>(
    spawner: T,
) -> BoxTaskSpawner 
where
    T: TaskSpawner,
{
    BoxTaskSpawner { factory: Box::new(spawner) }
}

pub trait TaskFactory: Send {
    type Fut: Future<Output = ()> + 'static;

    fn create_task(&self) -> Self::Fut;
}

impl<T, F> TaskFactory for T 
where
    T: Fn() -> F + Send,
    F: Future<Output = ()> + 'static
{
    type Fut = F;

    fn create_task(&self) -> Self::Fut {
        (self)()
    }
}

pub trait TaskSpawnerFactory: Send {
    type Spawner: TaskSpawner;

    fn create_spawner(&self) -> Self::Spawner;
}

impl<T, S> TaskSpawnerFactory for T 
where
    T: Fn() -> S + Send,
    S: TaskSpawner
{
    type Spawner = S;

    fn create_spawner(&self) -> Self::Spawner {
        (self)()
    }
}