use std::marker::PhantomData;

use super::{AspectContext, INextItem};
use anyhow::Result;

pub struct EmptyTask<T> {
    _pt: PhantomData<T>,
}

impl<T> EmptyTask<T> {
    pub fn new() -> Self {
        EmptyTask { _pt: PhantomData }
    }
}

impl<T> Clone for EmptyTask<T> {
    fn clone(&self) -> Self {
        Self {
            _pt: self._pt.clone(),
        }
    }
}

#[async_trait]
impl<T> INextItem<T> for EmptyTask<T>
where
    T: Send + Sync,
{
    async fn invoke_next(&self, _context: &mut AspectContext<T>) -> Result<()> {
        Ok(())
    }
}
