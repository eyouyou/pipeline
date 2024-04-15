//! 一个任务可以用多种方式进行结合
//! 1. 串行
//! 2. 并行
//! 3. [`Pipeline`] aop的方式进行结合 可以使用 [`Item`] 或者 [`Item`]、[`ItemThis`]
//!

use std::{any::Any, fmt::Debug, marker::PhantomData, sync::Arc};

pub mod parallel;
pub mod sequence;

use futures::future::BoxFuture;
pub use parallel::ParallelTask;
use pipeline::{
    AsAny, AspectContext, Downcast, INextItem, Item, ItemBox, NextItem, Pipeline, PipelineError,
    PipelineResult,
};

use self::sequence::SequenceTask;

pub trait IContext {
    fn merge(&mut self, other: &Self);
}

#[async_trait]
pub trait ITask<T>: dyn_clone::DynClone + AsAny {
    async fn invoke(&self, context: &mut AspectContext<T>) -> PipelineResult<()>;

    /// 失败的情况下 是否需要回滚等操作
    /// 因为回滚的操作 依赖于业务方invoke实现
    async fn rollback(&self, _: Option<&PipelineError>, _: &mut AspectContext<T>) {}
}

dyn_clone::clone_trait_object!(<T> ITask<T>);

#[async_trait]
pub(crate) trait ToItem<T> {
    // 常规任务 任务失败 则不继续走
    fn to_generic(self) -> ItemBox<T>;
    fn to_catch(self) -> ItemBox<T>;
}

struct LambdaTask<L, ContextT>
where
    L: Fn(&mut AspectContext<ContextT>) -> BoxFuture<PipelineResult<()>>,
{
    inner: L,
    _p: PhantomData<ContextT>,
}

impl<L, ContextT> Clone for LambdaTask<L, ContextT>
where
    L: Fn(&mut AspectContext<ContextT>) -> BoxFuture<PipelineResult<()>> + Clone,
{
    fn clone(&self) -> Self {
        LambdaTask {
            inner: self.inner.clone(),
            _p: PhantomData,
        }
    }
}

#[async_trait]
impl<L, ContextT> ITask<ContextT> for LambdaTask<L, ContextT>
where
    ContextT: Send + Sync + 'static,
    L: Fn(&mut AspectContext<ContextT>) -> BoxFuture<PipelineResult<()>> + Sync + Clone + 'static,
{
    async fn invoke(&self, context: &mut AspectContext<ContextT>) -> PipelineResult<()> {
        (self.inner)(context).await
    }
}

#[derive(Default)]
pub struct StrategyBuilder {
    catch: bool,
    retry: Option<u32>,
    retry_delay: Option<Arc<dyn Fn(u32) -> u64 + Send + Sync>>,
}

impl StrategyBuilder {
    fn build<T>(self, task: TaskBox<T>) -> StrategicTask<T> {
        StrategicTask {
            inner: task,
            catch: self.catch,
            retry: self.retry.unwrap_or(0),
            retry_delay: self.retry_delay.unwrap_or(Arc::new(|_| 0)),
        }
    }
}

impl StrategyBuilder {
    /// 设置是否捕获错误 如果捕获 即使当前[`Task`]支持错误也会继续往下走.
    ///
    /// 但是错误的回滚操作依赖于业务方实现 如果有错误 则会回滚 无论是否捕获.
    ///
    /// ## Returns Self 进行链式调用
    ///
    pub fn catch(&mut self) -> &mut Self {
        self.catch = true;
        self
    }

    /// 设置重试.
    ///
    /// ## Arguments
    ///
    /// * `retry` - 重试次数.
    /// * `retry_delay` - 重试间隔.
    ///
    /// ## Returns Self 进行链式调用
    ///
    pub fn retry<RD>(&mut self, retry: u32, retry_delay: RD) -> &mut Self
    where
        RD: Fn(u32) -> u64 + Send + Sync + 'static,
    {
        self.retry = Some(retry);
        self.retry_delay = Some(Arc::new(retry_delay));
        self
    }
}

pub type TaskBox<T> = Box<dyn ITask<T> + Send + Sync>;

impl<T> IChildrenFlow<T, StrategicTask<T>> for TaskBox<T>
where
    T: Clone + Send + 'static,
{
    fn children_as_mut(&mut self) -> Vec<&mut StrategicTask<T>> {
        let this = self.as_mut() as *mut (dyn ITask<T> + Send + Sync);

        // TODO: 通过unsafe绕过borrow checker
        unsafe {
            debug!("boxed {:?}, {:?} ", (*this).type_id(), (*this).type_name());
            debug!(
                "seq {:?}, {:?}",
                std::any::TypeId::of::<SequenceTask<T>>(),
                std::any::type_name::<SequenceTask<T>>(),
            );
            if let Some(s) = (*this).downcast_mut::<SequenceTask<T>>() {
                return s.children_as_mut();
            }

            if let Some(p) = (*this).downcast_mut::<ParallelTask<T>>() {
                return p.children_as_mut();
            }

            // (*this)
            //     .downcast_mut::<StrategicTask<T>>()
            //     .map_or_else(|| vec![], |x| vec![x])

            vec![]
        }
    }
}

pub trait IChildTask<T> {
    fn child_as_mut<Task>(&mut self) -> Option<&mut Task>
    where
        Task: 'static;
}

impl<T> IChildTask<T> for TaskBox<T>
where
    T: 'static,
{
    fn child_as_mut<Task>(&mut self) -> Option<&mut Task>
    where
        Task: 'static,
    {
        self.as_mut().downcast_mut::<Task>()
    }
}

#[derive(Clone)]
pub struct StrategicTask<T> {
    inner: TaskBox<T>,
    catch: bool,
    retry: u32,
    retry_delay: Arc<dyn Fn(u32) -> u64 + Send + Sync>,
}

impl<T> StrategicTask<T>
where
    T: Send + Clone + 'static,
{
    pub fn child_as_mut<Task>(&mut self) -> Option<&mut Task>
    where
        Task: 'static,
    {
        self.inner.as_mut().downcast_mut::<Task>()
    }
}

impl<T> Debug for StrategicTask<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "catch: {}, retry: {}", self.catch, self.retry)
    }
}

#[async_trait]
trait ItemThis<T> {
    async fn invoke_this(&self, context: &mut AspectContext<T>) -> PipelineResult<bool>;
}

#[async_trait]
impl<T> ItemThis<T> for StrategicTask<T>
where
    T: Send + 'static,
{
    async fn invoke_this(&self, context: &mut AspectContext<T>) -> PipelineResult<bool> {
        // 处理task主要逻辑
        let mut result = self.inner.invoke(context).await;

        let mut is_rollback = false;

        let mut retry = self.retry;

        // 一次retry失败 则会跟随一次回滚
        let mut index = 0;
        loop {
            if let Err(error) = &result {
                if true {
                    self.inner.rollback(Some(error), context).await;
                    is_rollback = true;
                }

                if retry == 0 {
                    break;
                }

                let delay = (self.retry_delay)(index);
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }

                result = self.inner.invoke(context).await;

                retry -= 1;
            } else {
                break;
            }

            index += 1;
        }

        if !self.catch {
            // 如果是catch任务 则不管结果如何 都调用next
            result?;
        }

        Ok(is_rollback)
    }
}

#[async_trait]
impl<T> Item<T> for StrategicTask<T>
where
    T: Send + Clone + 'static,
{
    async fn invoke(
        &self,
        next: &Box<NextItem<T>>,
        context: &mut AspectContext<T>,
    ) -> PipelineResult<()> {
        let is_rollback = self.invoke_this(context).await?;

        // 调用next
        let result = next.invoke_next(context).await;

        // 如果后面有任务执行完毕 且错误 且之前没有回滚过 则回滚
        if !is_rollback && result.is_err() {
            if true {
                self.inner.rollback(None, context).await;
            }
        }

        result
    }
}

#[async_trait]
impl<T> INextItem<T> for StrategicTask<T>
where
    T: Send + Clone + 'static,
{
    async fn invoke_next(&self, context: &mut AspectContext<T>) -> PipelineResult<()> {
        self.invoke_this(context).await.map(|_x| ())
    }
}

/// 任务流 不会是子任务
/// 如 [`SequenceTask`] [`ParallelTask`]
#[async_trait]
pub trait ITaskFlow<C> {
    async fn invoke(&self, context: &mut AspectContext<C>) -> PipelineResult<()>;
}

/// 策略任务流
pub struct StrategicTaskFlow<T> {
    inner: Pipeline<T>,
}

#[async_trait]
impl<C> ITask<C> for StrategicTaskFlow<C>
where
    C: Send + 'static,
{
    async fn invoke(&self, context: &mut AspectContext<C>) -> PipelineResult<()> {
        self.inner.invoke_next(context).await
    }
}

impl<T> Clone for StrategicTaskFlow<T> {
    fn clone(&self) -> Self {
        StrategicTaskFlow {
            inner: self.inner.clone(),
        }
    }
}

impl<T> StrategicTaskFlow<T>
where
    T: Send + Clone + 'static,
{
    pub fn new_builder<BT, Task>(inner: Task, builder: BT) -> Self
    where
        Task: TaskBoxed<T>,
        BT: Fn(&mut StrategyBuilder),
    {
        let mut b = StrategyBuilder::default();

        builder(&mut b);

        StrategicTaskFlow {
            inner: Pipeline::new(b.build(inner.boxed())),
        }
    }

    pub fn new<Task>(inner: Task) -> Self
    where
        Task: TaskBoxed<T>,
    {
        let b = StrategyBuilder::default();

        StrategicTaskFlow {
            inner: Pipeline::new(b.build(inner.boxed())),
        }
    }
}

impl<T> IChildrenFlow<T, StrategicTask<T>> for StrategicTaskFlow<T>
where
    T: Clone + Send + 'static,
{
    fn children_as_mut(&mut self) -> Vec<&mut StrategicTask<T>> {
        debug!(
            "StrategicTask<T> :{} {:?}",
            std::any::type_name::<StrategicTask<T>>(),
            std::any::TypeId::of::<StrategicTask<T>>()
        );

        self.inner
            .children_as_mut()
            .into_iter()
            .filter_map(|x| x.as_mut().downcast_mut())
            .collect()
    }
}

impl<T> StrategicTaskFlow<T>
where
    T: Send + Sync + Clone + 'static,
{
    pub fn simple() -> Self {
        StrategicTaskFlow {
            inner: Pipeline::default(),
        }
    }
}

/// 只能获取孩子的mutable的列表snapshot 不能修改孩子列表
pub trait IChildrenFlow<T, Task> {
    fn children_as_mut(&mut self) -> Vec<&mut Task>;
}

pub trait IStrategicTaskFlow<T>: IChildrenFlow<T, StrategicTask<T>> {
    fn then_builder_f<BT, Task>(
        &mut self,
        next: Task,
        builder_creator: BT,
    ) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>,
        BT: Fn(&mut StrategyBuilder),
    {
        let mut b = StrategyBuilder::default();
        builder_creator(&mut b);

        self.then_builder(next, b)?;
        Ok(self)
    }

    fn then<Task>(&mut self, next: Task) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>,
    {
        let b = StrategyBuilder::default();
        self.then_builder(next, b)?;

        Ok(self)
    }

    fn then_builder<Task>(
        &mut self,
        next: Task,
        builder: StrategyBuilder,
    ) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>;
}

impl<T> IStrategicTaskFlow<T> for StrategicTaskFlow<T>
where
    T: Send + Clone + 'static,
{
    fn then_builder<Task>(
        &mut self,
        next: Task,
        builder: StrategyBuilder,
    ) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>,
    {
        let boxed = next.boxed();

        self.inner.use_raw_item(builder.build(boxed));

        Ok(self)
    }
}

pub trait TaskBoxed<T>: AsAny {
    fn boxed(self) -> TaskBox<T>;
}

impl<T, Task> TaskBoxed<T> for Task
where
    Task: ITask<T> + Any + Send + Sync + 'static,
{
    fn boxed(self) -> TaskBox<T> {
        Box::new(self)
    }
}
