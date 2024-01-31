use std::{
    alloc::Layout,
    cmp::min,
    future,
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
    task::Poll
};

#[derive(Debug)]
pub struct Stack<T>
{
    state: AtomicU64,
    ptr: NonNull<T>,
}

#[derive(Clone, Debug)]
struct StackState
{
    pub capacity: u32,
    pub length: u32,
    pub locked: bool,
}
impl StackState
{
    const MASK_LOCK: u64 = 1 << (u64::BITS - 1);
    pub const MAX_LENGTH: usize = 1 << (u32::BITS - 1);
}

impl From<u64>
for StackState {
    fn from(
        value: u64
    ) -> Self {
        let sizes = value & !Self::MASK_LOCK;
        let cap = (sizes >> u32::BITS) as u32;
        let len = sizes as u32;

        debug_assert!((cap as usize) <= Self::MAX_LENGTH);
        assert!((len as usize) <= Self::MAX_LENGTH);

        Self {
            capacity: cap,
            length: len,
            locked: value & Self::MASK_LOCK != 0,
        }
    }
}

impl From<StackState>
for u64 {
    fn from(
        value: StackState
    ) -> Self {
        assert!((value.capacity as usize) <= StackState::MAX_LENGTH);
        assert!((value.length as usize) <= StackState::MAX_LENGTH);

        let lock = value.locked as u64 * StackState::MASK_LOCK;
        let cap = (value.capacity as u64) << u32::BITS;
        let len = value.length as u64;
        
        lock | cap | len
    }
}

impl<T> Stack<T>
{
    pub const MAX_LENGTH: usize = StackState::MAX_LENGTH;
    
    pub fn new(
        // no args
    ) -> Self {
        Self {
            state: Default::default(),
            ptr: NonNull::dangling(),
        }
    }

    pub fn with_capacity(
        capcacity: usize
    ) -> Option<Self> {
        assert!(capcacity <= Self::MAX_LENGTH);

        let layout = std::alloc::Layout::array::<T>(capcacity).expect("Attempt to allocate internal buffer, with an invalid memory layout");
        let raw_ptr = unsafe { std::alloc::alloc(layout) as *mut T };

        NonNull::new(raw_ptr).map(|valid_ptr| {
            let state = StackState{
                capacity: capcacity as u32,
                length: 0,
                locked: false,
            };

            Self {
                state: AtomicU64::new(state.into()),
                ptr: valid_ptr,
            }
        })
    }

    pub async fn push(
        &mut self,
        item: T
    ) {
        // Get the next index and lock the vec
        let locked_state: StackState = {
            let mut old_state: StackState = self.state.load(Ordering::Relaxed).into();

            future::poll_fn(|_| {
                let (mut desired_old, mut desired_new) = (old_state.clone(), old_state.clone());
                desired_old.locked = false;

                desired_new.locked = true;
                desired_new.length += 1;

                match self.state.compare_exchange(
                    desired_old.into(),
                    desired_new.into(),
                    Ordering::Release,
                    Ordering::Relaxed
                ) {
                    Ok(state) => Poll::Ready(state.into()),
                    Err(state) => {
                        old_state = state.into();
                        Poll::Pending
                    }
                }
            }).await
        };

        let index = locked_state.length - 1;
        
        // Increase vec capacity (if required)
        if locked_state.capacity <= index {
            if locked_state.capacity == 0 {
                let ptr = unsafe {
                    std::alloc::alloc(
                        Layout::array::<T>(1).expect("Single element array is an invalid layout")
                    )
                } as *mut T;
                self.ptr = NonNull::new(ptr).expect("Internal buffer allocation failed");
            }
            else {
                let new_size = min(locked_state.capacity as usize * 2, Self::MAX_LENGTH);
                if new_size == locked_state.capacity as usize {
                    panic!("Attempted to exceed maximum capacity")
                }

                let (old_layout, new_layout) = (
                    Layout::array::<T>(locked_state.capacity as usize)
                        .expect("Memory layout that is already in use, is now invalid"),
                    Layout::array::<T>(new_size)
                        .expect("Attempting to reallocate internal buffer with an invalid memory layout")
                );
                
                let new_ptr = unsafe {
                    std::alloc::realloc(
                        self.ptr.as_ptr() as *mut u8,
                        old_layout,
                        new_layout.size()
                    )
                } as *mut T;

                self.ptr = NonNull::new(new_ptr).expect("Reallocation of internal buffer failed");
            }
        }
        
        // Set our value in the internal buffer
        let target = unsafe {
            self.ptr.as_ptr()
                .offset(index as isize)
                .as_mut()
                .expect("Null pointer dereferenced")
        };
        *target = item;

        // Unlock our state
        future::poll_fn(|_| {
            let (mut desired_old, mut desired_new) = (locked_state.clone(), locked_state.clone());
            desired_old.locked = true;
            desired_new.locked = false;

            match self.state.compare_exchange(
                desired_old.into(),
                desired_new.into(),
                Ordering::Release,
                Ordering::Relaxed
            ) {
                Ok(_) => Poll::Ready(()),
                Err(_lock_mutated) => panic!("Locked resource was mutated outside of held lock"),
            }
        }).await
    }

    pub async fn pop(
        &mut self
    ) -> Option<T> {
        // Get the next index and lock the vec
        let locked_state: StackState = {
            let mut old_state: StackState = self.state.load(Ordering::Relaxed).into();

            future::poll_fn(|_| {
                let (mut desired_old, mut desired_new) = (old_state.clone(), old_state.clone());
                desired_old.locked = false;
                desired_new.locked = true;

                match self.state.compare_exchange(
                    desired_old.into(),
                    desired_new.into(),
                    Ordering::Release,
                    Ordering::Relaxed
                ) {
                    Ok(state) => Poll::Ready(state.into()),
                    Err(state) => {
                        old_state = state.into();
                        Poll::Pending
                    }
                }
            }).await
        };

        let index = std::cmp::max(0, (locked_state.length - 1) as isize);
        
        // Get our value from internal buffer
        let result = match locked_state.length {
            0 => None,
            _ => {
                let target = unsafe {
                    self.ptr.as_ptr()
                        .offset(index)
                        .as_mut()
                        .map(|t| std::mem::replace(t, std::mem::MaybeUninit::<T>::uninit().assume_init_read()))
                        .expect("Null pointer dereferenced")
                };
                Some(target)
            },
        };

        // Unlock our state
        future::poll_fn(|_| {
            let (mut desired_old, mut desired_new) = (locked_state.clone(), locked_state.clone());
            desired_old.locked = true;
            desired_new.locked = false;
            desired_new.length = index as u32;

            match self.state.compare_exchange(
                desired_old.into(),
                desired_new.into(),
                Ordering::Release,
                Ordering::Relaxed
            ) {
                Ok(_) => Poll::Ready(()),
                Err(_lock_mutated) => panic!("Locked resource was mutated outside of held lock"),
            }
        }).await;

        result
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}
