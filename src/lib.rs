//! 通用拦截器
//!
//! 每次调用都会产生所有管道拦截器的拷贝动作 相当于所有状态不能在拦截器中保存
//!
//! 函数式存储模型
//! 只能在context中进行存储 旨在除了context 所有拦截器幂等
//!
//! 高阶设计模型
//! add => 1 2 3
//! Z(next) => Y(context) => X(next, context)
//!  ⬇
//! Z1(Z2(Z3(next)))(context)
//!

extern crate anyhow;
use anyhow::Result;
pub use anyhow::{anyhow, bail};

#[macro_use]
extern crate async_trait;

mod default;
use default::DefaultController;
use dyn_clone::DynClone;

pub struct Context<T> {
    pub current: T,
}

/// 不需要实现
#[async_trait]
pub trait INextController<T>: DynClone
where
    Self: Send,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()>;
}

pub type UnsyncNextController<T> = dyn INextController<T>;
pub type NextController<T> = dyn INextController<T> + Sync;

dyn_clone::clone_trait_object!(<T> INextController<T>);

struct NextPipeTask<Context> {
    inner: Box<dyn Interceptor<Context>>,
    next: Box<UnsyncNextController<Context>>,
}

impl<T> Clone for NextPipeTask<T> {
    fn clone(&self) -> Self {
        NextPipeTask {
            inner: self.inner.clone(),
            next: self.next.clone(),
        }
    }
}

#[async_trait]
impl<T> INextController<T> for NextPipeTask<T>
where
    T: Send,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()> {
        self.inner.invoke(&mut self.next, context).await
    }
}

#[async_trait]
pub trait Interceptor<T>: DynClone
where
    Self: Send,
{
    async fn invoke(
        &mut self,
        next: &mut Box<UnsyncNextController<T>>,
        context: &mut Context<T>,
    ) -> Result<()>;
}

dyn_clone::clone_trait_object!(<T> Interceptor<T>);

pub struct Pipeline<T> {
    end: Box<NextController<T>>,
    stack: Vec<Box<dyn Interceptor<T> + Sync>>,
}

impl<Context> Default for Pipeline<Context>
where
    Context: Send + Sync + 'static,
{
    /// 默认管线 无需进行核心逻辑，只是为了支持默认
    fn default() -> Self {
        Pipeline {
            end: Box::new(DefaultController::new()),
            stack: vec![],
        }
    }
}

impl<Context> Pipeline<Context> {
    /// 支持处理核心逻辑之前添加管线处理
    pub fn new<Inner>(inner: Inner) -> Self
    where
        Inner: INextController<Context> + Sync + 'static,
    {
        Pipeline {
            end: Box::new(inner),
            stack: vec![],
        }
    }
}

impl<T> Pipeline<T> {
    pub fn use_interceptor<Task>(&mut self, next: Task)
    where
        Task: Interceptor<T> + Sync + 'static,
    {
        self.stack.insert(0, Box::new(next));
    }
}

impl<T> Clone for Pipeline<T> {
    fn clone(&self) -> Self {
        Pipeline {
            end: self.end.clone(),
            stack: self.stack.clone(),
        }
    }
}

#[async_trait]
impl<C> INextController<C> for Pipeline<C>
where
    C: Send + 'static,
{
    async fn invoke(&mut self, context: &mut Context<C>) -> Result<()> {
        let mut moved: Box<UnsyncNextController<C>> = dyn_clone::clone_box(&*self.end);
        for item in self.stack.iter_mut() {
            moved = Box::new(NextPipeTask {
                inner: item.clone(),
                next: moved,
            });
        }
        moved.invoke(context).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::UnsyncNextController;

    use super::{Context, INextController, Interceptor, Pipeline};

    #[derive(Debug)]
    struct Data {
        tag: String,
    }

    #[derive(Clone)]
    struct TaskA {}

    #[async_trait]
    impl Interceptor<Data> for TaskA {
        async fn invoke(
            &mut self,
            next: &mut Box<UnsyncNextController<Data>>,
            context: &mut Context<Data>,
        ) -> Result<()> {
            println!("start a");
            next.invoke(context).await?;
            println!("end a");
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TaskB {}

    #[async_trait]
    impl Interceptor<Data> for TaskB {
        async fn invoke(
            &mut self,
            next: &mut Box<UnsyncNextController<Data>>,
            context: &mut Context<Data>,
        ) -> Result<()> {
            println!("start b");
            next.invoke(context).await?;
            println!("end b");
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TaskC {}

    #[async_trait]
    impl Interceptor<Data> for TaskC {
        async fn invoke(
            &mut self,
            next: &mut Box<UnsyncNextController<Data>>,
            context: &mut Context<Data>,
        ) -> Result<()> {
            println!("start c");
            next.invoke(context).await?;
            context.current.tag = "c".to_owned();
            println!("end c");
            Ok(())
        }
    }

    #[tokio::test]
    async fn test() {
        let mut context = Context {
            current: Data { tag: "".to_owned() },
        };
        let mut line = Pipeline::default();
        line.use_interceptor(TaskA {});
        line.use_interceptor(TaskB {});
        line.use_interceptor(TaskC {});

        _ = line.invoke(&mut context).await;

        assert_eq!(context.current.tag, "c".to_owned());
    }
}
