use std::marker::PhantomData;

use anyhow::Result;

use super::{Context, INextController};

pub struct DefaultPipe<T> {
    _pt: PhantomData<T>,
}

unsafe impl<Context> Send for DefaultPipe<Context> {}
unsafe impl<Context> Sync for DefaultPipe<Context> {}

impl<T> DefaultPipe<T> {
    pub fn new() -> Self {
        DefaultPipe { _pt: PhantomData }
    }
}

impl<T> Clone for DefaultPipe<T> {
    fn clone(&self) -> Self {
        Self {
            _pt: self._pt.clone(),
        }
    }
}

#[async_trait]
impl<T> INextController<T> for DefaultPipe<T>
where
    T: Send,
{
    async fn invoke(&mut self, _context: &mut Context<T>) -> Result<()> {
        Ok(())
    }
}
