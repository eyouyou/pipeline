//! fallback 链
//! 多级备用
//!
//!

use pipeline::{AspectContext, INextItem, Item, NextItem, Pipeline, PipelineResult};

use super::{TaskBox, TaskBoxed};

pub trait IFallback<T> {
    fn then<Task>(&mut self, next: Task) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>;
}

#[derive(Clone)]
pub struct Fallback<T> {
    tasks: Pipeline<T>,
}

impl<T> Fallback<T>
where
    T: Send + Sync + 'static,
{
    pub fn new() -> Self {
        Fallback {
            tasks: Pipeline::default(),
        }
    }
}

impl<T> Fallback<T>
where
    T: Send + 'static,
{
    pub async fn invoke(&self, context: &mut AspectContext<T>) -> PipelineResult<()> {
        self.tasks.invoke_next(context).await
    }
}

impl<T> IFallback<T> for Fallback<T>
where
    T: Send + Clone + 'static,
{
    fn then<Task>(&mut self, next: Task) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<T>,
    {
        self.tasks.use_raw_item(FallbackTask {
            inner: next.boxed(),
        });

        Ok(self)
    }
}

#[derive(Clone)]
struct FallbackTask<T> {
    inner: TaskBox<T>,
}

#[async_trait]
impl<T> Item<T> for FallbackTask<T>
where
    T: Send + Clone + 'static,
{
    async fn invoke(
        &self,
        next: &Box<NextItem<T>>,
        context: &mut AspectContext<T>,
    ) -> PipelineResult<()> {
        // 如果当前fallback 调用失败 则调用下一个
        if let Err(_) = self.inner.invoke(context).await {
            next.invoke_next(context).await?;
        };

        Ok(())
    }
}
