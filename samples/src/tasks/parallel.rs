//! 并行任务
//! 可以走多个分支
//! 多分支全部走完才能走下一个
//! TODO: 分支如果出现错误 是否需要同步上下文 暂时同步

use std::collections::HashMap;

use futures::{
    future::{join_all, select_all},
    FutureExt,
};
use pipeline::{AspectContext, PipelineError, PipelineResult};

use super::{sequence::SequenceTask, IChildrenFlow, IContext, ITask, StrategicTask};

#[derive(Clone)]
pub struct ParallelTask<ContextT> {
    nexts: HashMap<String, SequenceTask<ContextT>>,
    condition: fn(&AspectContext<ContextT>) -> Vec<String>,
    await_next_all: bool,
}

impl<ContextT> ParallelTask<ContextT> {
    pub fn new(
        condition: fn(&AspectContext<ContextT>) -> Vec<String>,
        await_next_all: bool,
    ) -> Self {
        ParallelTask {
            nexts: HashMap::new(),
            condition,
            await_next_all,
        }
    }
}

impl<T> IChildrenFlow<T, StrategicTask<T>> for ParallelTask<T>
where
    T: Clone + Send + 'static,
{
    fn children_as_mut(&mut self) -> Vec<&mut StrategicTask<T>> {
        self.nexts
            .values_mut()
            .flat_map(|x| x.children_as_mut())
            .collect()
    }
}

pub trait ParallelTaskEx<ContextT> {
    fn branch(&mut self, branch: String, next: SequenceTask<ContextT>) -> &mut Self;
}

impl<ContextT> ParallelTaskEx<ContextT> for ParallelTask<ContextT> {
    fn branch(&mut self, branch: String, next: SequenceTask<ContextT>) -> &mut Self {
        self.nexts.insert(branch, next);
        self
    }
}

async fn process_line<ContextT>(
    line: &mut SequenceTask<ContextT>,
    mut context: AspectContext<ContextT>,
) -> (PipelineResult<()>, AspectContext<ContextT>)
where
    ContextT: Send + Clone + 'static,
{
    let result = line.invoke(&mut context).await;

    (result, context)
}

#[async_trait]
impl<ContextT> ITask<ContextT> for ParallelTask<ContextT>
where
    ContextT: IContext + Send + Clone + 'static,
{
    async fn invoke(&self, context: &mut AspectContext<ContextT>) -> PipelineResult<()> {
        let result = (self.condition)(context);
        let mut lines: HashMap<String, SequenceTask<ContextT>> = result
            .iter()
            .filter_map(|x| self.nexts.get(x).map(|line| (x.clone(), line.clone())))
            .collect();

        let all = lines
            .iter_mut()
            .map(|(_, line)| process_line(line, context.clone()).boxed());

        match self.await_next_all {
            true => {
                let results = join_all(all).await;
                let mut error_results = Vec::new();
                for (result, c) in results {
                    context.current.merge(&c.current);

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            error_results.push(e);
                        }
                    }
                }
                if error_results.len() > 0 {
                    Err(PipelineError::Aggregated(error_results))
                } else {
                    Ok(())
                }
            }
            false => {
                let ((result, c), _, _) = select_all(all).await;
                context.current.merge(&c.current);

                result?;
                Ok(())
            }
        }?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use pipeline::{AspectContext, PipelineError, PipelineResult};

    use crate::tasks::{
        parallel::ParallelTaskEx, sequence::SequenceTask, IStrategicTaskFlow, ITask, ParallelTask,
        StrategicTaskFlow,
    };

    #[derive(Debug, Clone)]
    struct Data {
        tag: String,
    }

    impl super::IContext for Data {
        fn merge(&mut self, other: &Self) {
            self.tag = other.tag.clone();
        }
    }

    #[derive(Clone)]
    struct TaskA {}

    #[async_trait]
    impl ITask<Data> for TaskA {
        async fn invoke(&self, _: &mut AspectContext<Data>) -> PipelineResult<()> {
            println!("start a");
            println!("end a");
            Ok(())
        }

        async fn rollback(&self, _: Option<&PipelineError>, _: &mut AspectContext<Data>) {
            println!("fallback a");
        }
    }

    #[derive(Clone)]
    struct TaskB {}

    #[async_trait]
    impl ITask<Data> for TaskB {
        async fn invoke(&self, _: &mut AspectContext<Data>) -> PipelineResult<()> {
            println!("start b");

            println!("end b");
            Ok(())
        }
        async fn rollback(&self, _: Option<&PipelineError>, _: &mut AspectContext<Data>) {
            println!("fallback b");
        }
    }

    #[derive(Clone)]
    struct TaskC1 {}

    #[async_trait]
    impl ITask<Data> for TaskC1 {
        async fn invoke(&self, _: &mut AspectContext<Data>) -> PipelineResult<()> {
            println!("start c1");
            println!("end c1");
            Ok(())
        }

        async fn rollback(&self, _: Option<&PipelineError>, _: &mut AspectContext<Data>) {
            println!("fallback c1");
        }
    }

    #[derive(Clone)]
    struct TaskC2 {}

    #[async_trait]
    impl ITask<Data> for TaskC2 {
        async fn invoke(&self, context: &mut AspectContext<Data>) -> PipelineResult<()> {
            println!("start c2");
            context.current.tag = "c".to_owned();
            println!("end c2");
            Err(anyhow!("error").into())
        }
        async fn rollback(&self, _: Option<&PipelineError>, _: &mut AspectContext<Data>) {
            println!("fallback c2");
        }
    }

    #[derive(Clone)]
    struct TaskD {}

    #[async_trait]
    impl ITask<Data> for TaskD {
        async fn invoke(&self, _: &mut AspectContext<Data>) -> PipelineResult<()> {
            println!("start d");
            println!("end d");
            Ok(())
        }
    }

    #[tokio::test]
    async fn test() -> anyhow::Result<()> {
        let mut context = AspectContext::new(Data { tag: "".to_owned() });
        let mut line = StrategicTaskFlow::simple();
        line.then(TaskA {})?;

        let mut line_b = SequenceTask::new();
        line_b.then_builder_f(TaskB {}, |x| {
            x.retry(1, |x| (x * 100).into());
        })?;
        let mut line_c = SequenceTask::new();
        line_c.then(TaskC1 {})?.then(TaskC2 {})?;

        let mut parallel = ParallelTask::new(|_| vec!["c".to_string()], true);

        parallel.branch("b".to_string(), line_b);
        parallel.branch("c".to_string(), line_c);

        line.then(parallel)?.then(TaskD {})?;

        _ = line.invoke(&mut context).await;

        assert_eq!(context.current.tag, "c".to_owned());

        Ok(())
    }
}
