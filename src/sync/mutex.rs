use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};

#[derive(Debug)]
pub struct AsyncMut<T> {
    pub(in self) holder: AtomicUsize,
    next: AtomicUsize,
    pub(in self) data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send
for AsyncMut<T> { }

unsafe impl<T> Sync
for AsyncMut<T> { }

impl<'me, T> AsyncMut<T>
{
    pub fn new(
        init: T
    ) -> Self {
        Self {
            holder: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
            data: UnsafeCell::new(init),
        }
    }

    pub async fn lock(
        &'me self
    ) -> impl 'me + std::ops::Deref<Target = T> + std::ops::DerefMut + Drop {
        let my_token = {
            let mut tmp_token: usize = 0;
            std::future::poll_fn(move |_| {
                match self.next.compare_exchange(
                    tmp_token,
                    tmp_token + 1,
                    Ordering::Release,
                    Ordering::Relaxed
                ) {
                    Ok(mine) => Poll::Ready(mine),
                    Err(current) => {
                        tmp_token = current;
                        Poll::Pending
                    },
                }
            }).await
        };

        std::future::poll_fn(move |_| match self.holder.compare_exchange(
            my_token,
            my_token,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(my_token) => Poll::Ready(Lock::new(
                self,
                my_token
            )),
            Err(_held) => Poll::Pending,
        }).await
    }
}

struct Lock<'owner, T>
{
    owner: &'owner AsyncMut<T>,
    token: usize,
}

impl<'owner, T>
Lock<'owner, T>
{
    pub fn new(
        owner: &'owner AsyncMut<T>,
        token: usize
    ) -> Self {
        Self{ owner, token }
    }
}

impl<'owner, T> std::ops::Deref
for Lock<'owner, T>
{
    type Target = T;

    fn deref(
        &self
    ) -> &Self::Target {
        unsafe { & *self.owner.data.get() }
    }
}

impl<'owner, T> std::ops::DerefMut
for Lock<'owner, T>
{
    fn deref_mut(
        &mut self
    ) -> &mut Self::Target {
        unsafe { &mut *self.owner.data.get() }
    }
}

impl<'owner, T> Drop
for Lock<'owner, T>
{
    fn drop(
        &mut self
    ) {
        // spin lock drop

        while let Err(_other) = self.owner.holder.compare_exchange(
        self.token,
            self.token + 1, 
        Ordering::Release,
        Ordering::Relaxed
        ) {
            continue;
        }
    }
}
