# TaskFlow sample

## Fallback

fallback 链 多级备用

## ParallelTask

可以走多个分支 多线程推进 多分支全部走完才能走下一个

## SequenceTask

序列任务 串行推进

[ParallelTask AND SequenceTask](./src/tasks/parallel.rs)

``` rust
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
        // init task flow and task context
        let mut context = AspectContext::new(Data { tag: "".to_owned() });
        let mut line = StrategicTaskFlow::simple();
        line.then(TaskA {})?;

        // branch b
        let mut line_b = SequenceTask::new();
        line_b.then_builder_f(TaskB {}, |x| {
            x.retry(1, |x| (x * 100).into());
        })?;

        // branch c
        let mut line_c = SequenceTask::new();
        line_c.then(TaskC1 {})?.then(TaskC2 {})?;

        // parallel task
        let mut parallel = ParallelTask::new(|_| vec!["c".to_string()], true);

        // bind branch
        parallel.branch("b".to_string(), line_b);
        parallel.branch("c".to_string(), line_c);

        line.then(parallel)?.then(TaskD {})?;

        // exec task
        _ = line.invoke(&mut context).await;

        assert_eq!(context.current.tag, "c".to_owned());

        Ok(())
    }
```
