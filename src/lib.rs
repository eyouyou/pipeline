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
use default::DefaultPipe;
use dyn_clone::DynClone;
use std::marker::PhantomData;

pub struct Context<T> {
    data: T,
}

/// 不需要实现
#[async_trait]
pub trait INextController<T>: DynClone
where
    Self: Send + Sync,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()>;
}

dyn_clone::clone_trait_object!(<T> INextController<T>);

#[derive(Clone)]
struct NextPipeTask<T>
where
    T: Clone,
{
    inner: Box<dyn Interceptor<T>>,
    next: Box<dyn INextController<T>>,
    _p: PhantomData<T>,
}

#[async_trait]
impl<T> INextController<T> for NextPipeTask<T>
where
    T: Send + Sync + Clone,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()> {
        self.inner.invoke(&mut self.next, context).await
    }
}

#[async_trait]
pub trait Interceptor<T>: DynClone
where
    Self: Send + Sync,
{
    async fn invoke(
        &mut self,
        next: &mut Box<dyn INextController<T>>,
        context: &mut Context<T>,
    ) -> Result<()>;
}

dyn_clone::clone_trait_object!(<T> Interceptor<T>);

#[derive(Clone)]
pub struct Pipeline<T>
where
    T: Clone,
{
    end: Box<dyn INextController<T>>,
    stack: Vec<Box<dyn Interceptor<T>>>,
    _pt: PhantomData<T>,
}

impl<T> Pipeline<T>
where
    T: Send + Sync + Clone + 'static,
{
    /// 默认管线 无需进行核心逻辑，只是为了支持默认
    pub fn default() -> Self {
        Pipeline {
            end: Box::new(DefaultPipe::<T>::new()),
            stack: vec![],
            _pt: PhantomData,
        }
    }

    /// 支持处理核心逻辑之前添加管线处理
    pub fn new<Inner>(inner: Inner) -> Self
    where
        Inner: INextController<T> + 'static,
    {
        Pipeline {
            end: Box::new(inner),
            stack: vec![],
            _pt: PhantomData,
        }
    }
}

impl<T> Pipeline<T>
where
    T: Clone,
{
    pub fn use_interceptor<Task>(&mut self, next: Task)
    where
        Task: Interceptor<T> + 'static,
    {
        self.stack.insert(0, Box::new(next));
    }
}

#[async_trait]
impl<T> INextController<T> for Pipeline<T>
where
    T: Send + Sync + Clone + 'static,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()> {
        let mut moved: Box<dyn INextController<T>> = dyn_clone::clone_box(&*self.end);
        for item in self.stack.iter_mut() {
            moved = Box::new(NextPipeTask {
                inner: item.clone(),
                next: moved,
                _p: PhantomData,
            });
        }
        moved.invoke(context).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{Context, INextController, Interceptor, Pipeline};

    #[derive(Debug, Clone)]
    struct Data {
        tag: String,
    }

    #[derive(Clone)]
    struct TaskA {}

    #[async_trait]
    impl Interceptor<Data> for TaskA {
        async fn invoke(
            &mut self,
            next: &mut Box<dyn INextController<Data>>,
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
            next: &mut Box<dyn INextController<Data>>,
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
            next: &mut Box<dyn INextController<Data>>,
            context: &mut Context<Data>,
        ) -> Result<()> {
            println!("start c");
            next.invoke(context).await?;
            context.data.tag = "c".to_owned();
            println!("end c");
            Ok(())
        }
    }

    #[tokio::test]
    async fn test() {
        let mut context = Context {
            data: Data { tag: "".to_owned() },
        };
        let mut line = Pipeline::default();
        line.use_interceptor(TaskA {});
        line.use_interceptor(TaskB {});
        line.use_interceptor(TaskC {});

        _ = line.invoke(&mut context).await;

        assert_eq!(context.data.tag, "c".to_owned());
    }
}
