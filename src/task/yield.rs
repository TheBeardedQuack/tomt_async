use std::{future::Future, task::Poll};

pub(crate) struct Yield{
    pub yielded: bool
}

impl Future
for Yield
{
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<Self::Output> {
        match self.yielded {
            false => {
                self.get_mut().yielded = true;
                cx.waker().wake_by_ref();

                Poll::Pending
            },
            true => Poll::Ready(()),
        }
    }
}
