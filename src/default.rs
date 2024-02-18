use std::marker::PhantomData;

use anyhow::Result;

use super::{Context, INextController};

pub struct DefaultController<T> {
    _pt: PhantomData<T>,
}

impl<T> DefaultController<T> {
    pub fn new() -> Self {
        DefaultController { _pt: PhantomData }
    }
}

impl<T> Clone for DefaultController<T> {
    fn clone(&self) -> Self {
        Self {
            _pt: self._pt.clone(),
        }
    }
}

#[async_trait]
impl<T> INextController<T> for DefaultController<T>
where
    T: Send,
{
    async fn invoke(&mut self, _context: &mut Context<T>) -> Result<()> {
        Ok(())
    }
}
