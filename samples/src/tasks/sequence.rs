//! 序列任务

use pipeline::{AspectContext, PipelineResult};

use super::{IChildrenFlow, IStrategicTaskFlow, ITask, StrategicTask, StrategicTaskFlow, StrategyBuilder, TaskBoxed};

#[derive(Clone)]
pub struct SequenceTask<ContextT> {
    tasks: StrategicTaskFlow<ContextT>,
}

impl<ContextT> SequenceTask<ContextT>
where
    ContextT: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        SequenceTask {
            tasks: StrategicTaskFlow::simple(),
        }
    }
}

impl<T> IChildrenFlow<T, StrategicTask<T>> for SequenceTask<T>
where
    T: Clone + Send + 'static,
{
    fn children_as_mut(&mut self) -> Vec<&mut StrategicTask<T>> {
        self.tasks.children_as_mut()
    }
}

impl<ContextT> IStrategicTaskFlow<ContextT> for SequenceTask<ContextT>
where
    ContextT: Send + Clone + 'static,
{
    fn then_builder<Task>(&mut self, next: Task, builder: StrategyBuilder) -> PipelineResult<&mut Self>
    where
        Task: TaskBoxed<ContextT>,
    {
        self.tasks.then_builder(next, builder)?;

        Ok(self)
    }
}

#[async_trait]
impl<ContextT> ITask<ContextT> for SequenceTask<ContextT>
where
    ContextT: Send + Clone + 'static,
{
    async fn invoke(&self, context: &mut AspectContext<ContextT>) -> PipelineResult<()> {
        self.tasks.invoke(context).await
    }
}
