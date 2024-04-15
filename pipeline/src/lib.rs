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

mod as_any;
mod default;
mod error;

#[macro_use]
extern crate async_trait;
use anyhow::anyhow;
pub use as_any::{AsAny, Downcast};
use async_trait::async_trait;
use default::EmptyTask;
use dyn_clone::DynClone;
pub use error::{PipelineError, PipelineResult};

/// 切面上下文
pub struct AspectContext<T> {
    pub current: T,
    continued: Option<()>,
}

impl<T> Clone for AspectContext<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        AspectContext {
            current: self.current.clone(),
            continued: self.continued.clone(),
        }
    }
}

#[async_trait]
pub trait IContext {
    fn check_canceled(&mut self) -> PipelineResult<()>;
    async fn set_canceled(&mut self);
}

impl<T> AspectContext<T> {
    pub fn new(data: T) -> Self {
        AspectContext {
            current: data,
            continued: Some(()),
        }
    }
}

#[async_trait]
impl<T> IContext for AspectContext<T>
where
    T: Send,
{
    fn check_canceled(&mut self) -> PipelineResult<()> {
        self.continued.ok_or(anyhow!("canceled").into())
    }

    async fn set_canceled(&mut self) {
        self.continued = None;
    }
}

/// 不需要实现
#[async_trait]
pub trait INextItem<T>: DynClone
where
    Self: Send + Sync,
{
    async fn invoke_next(&self, context: &mut AspectContext<T>) -> PipelineResult<()>;
}

pub type UnsyncNextItem<T> = dyn INextItem<T>;
pub type NextItem<T> = dyn INextItem<T> + Sync;

dyn_clone::clone_trait_object!(<T> INextItem<T>);

struct NextPipeTask<Context> {
    inner: ItemBox<Context>,
    next: Box<NextItem<Context>>,
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
impl<T> INextItem<T> for NextPipeTask<T>
where
    T: Send + 'static,
{
    async fn invoke_next(&self, context: &mut AspectContext<T>) -> PipelineResult<()> {
        context.check_canceled()?;
        self.inner.invoke(&self.next, context).await
    }
}

#[async_trait]
pub trait Item<T>: DynClone + AsAny
where
    Self: Send,
{
    async fn invoke(
        &self,
        next: &Box<NextItem<T>>,
        context: &mut AspectContext<T>,
    ) -> PipelineResult<()>;
}

dyn_clone::clone_trait_object!(<T> Item<T>);

pub struct Pipeline<T> {
    end: Box<NextItem<T>>,
    stack: Vec<ItemBox<T>>,
}

impl<Context> Default for Pipeline<Context>
where
    Context: Send + Sync + 'static,
{
    /// 默认管线 无需进行核心逻辑，只是为了支持默认
    fn default() -> Self {
        Pipeline {
            end: Box::new(EmptyTask::new()),
            stack: vec![],
        }
    }
}

impl<Context> Pipeline<Context> {
    /// 支持处理核心逻辑之前添加管线处理
    pub fn new<Inner>(inner: Inner) -> Self
    where
        Inner: INextItem<Context> + Sync + 'static,
    {
        Pipeline {
            end: Box::new(inner),
            stack: vec![],
        }
    }
}

impl<T> Pipeline<T> {
    pub fn use_raw_item<I>(&mut self, next: I)
    where
        I: Item<T> + 'static + Sync,
    {
        self.stack.insert(0, Box::new(next));
    }

    pub fn use_item(&mut self, next: ItemBox<T>) {
        self.stack.insert(0, next);
    }

    pub fn children_as_mut(&mut self) -> Vec<&mut ItemBox<T>> {
        let mut vec: Vec<_> = self.stack.iter_mut().collect();

        vec.reverse();

        vec
    }
}

pub type ItemBox<T> = Box<dyn Item<T> + Sync>;

impl<T> Clone for Pipeline<T> {
    fn clone(&self) -> Self {
        Pipeline {
            end: self.end.clone(),
            stack: self.stack.clone(),
        }
    }
}

#[async_trait]
impl<C> INextItem<C> for Pipeline<C>
where
    C: Send + 'static,
{
    async fn invoke_next(&self, context: &mut AspectContext<C>) -> PipelineResult<()> {
        let mut moved: Box<NextItem<C>> = dyn_clone::clone_box(&*self.end);
        for item in self.stack.iter() {
            moved = Box::new(NextPipeTask {
                inner: item.clone(),
                next: moved,
            });
        }
        moved.invoke_next(context).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::PipelineResult;

    use super::{AspectContext, INextItem, Item, NextItem, Pipeline};

    #[derive(Debug)]
    struct Data {
        tag: String,
    }

    #[derive(Clone)]
    struct TaskA {}

    #[async_trait]
    impl Item<Data> for TaskA {
        async fn invoke(
            &self,
            next: &Box<NextItem<Data>>,
            context: &mut AspectContext<Data>,
        ) -> PipelineResult<()> {
            println!("start a");
            next.invoke_next(context).await?;
            println!("end a");
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TaskB {}

    #[async_trait]
    impl Item<Data> for TaskB {
        async fn invoke(
            &self,
            next: &Box<NextItem<Data>>,
            context: &mut AspectContext<Data>,
        ) -> PipelineResult<()> {
            println!("start b");
            next.invoke_next(context).await?;
            println!("end b");
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TaskC {}

    #[async_trait]
    impl Item<Data> for TaskC {
        async fn invoke(
            &self,
            next: &Box<NextItem<Data>>,
            context: &mut AspectContext<Data>,
        ) -> PipelineResult<()> {
            println!("start c");
            next.invoke_next(context).await?;
            context.current.tag = "c".to_owned();
            println!("end c");
            Ok(())
        }
    }

    #[tokio::test]
    async fn test() {
        let mut context = AspectContext::new(Data { tag: "".to_owned() });
        let mut line = Pipeline::default();
        line.use_raw_item(TaskA {});
        line.use_raw_item(TaskB {});
        line.use_raw_item(TaskC {});

        _ = line.invoke_next(&mut context).await;

        assert_eq!(context.current.tag, "c".to_owned());
    }
}
