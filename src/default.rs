use std::marker::PhantomData;

use anyhow::Result;

use super::{Context, INextController};

#[derive(Clone)]
pub struct DefaultPipe<T> {
    _pt: PhantomData<T>,
}

impl<T> DefaultPipe<T> {
    pub fn new() -> Self {
        DefaultPipe { _pt: PhantomData }
    }
}

#[async_trait]
impl<T> INextController<T> for DefaultPipe<T>
where
    T: Send + Sync + Clone,
{
    async fn invoke(&mut self, context: &mut Context<T>) -> Result<()> {
        Ok(())
    }
}
