use std::{
    ptr::null_mut,
    sync::{
        atomic::{AtomicPtr, Ordering},
        Arc,
    },
};

#[derive(Debug)]
pub struct APArc<T> {
    ptr: AtomicPtr<T>,
}

impl<T> APArc<T> {
    pub fn new() -> APArc<T> {
        APArc { ptr: AtomicPtr::<T>::default() }
    }

    pub fn swap_null(&self, arc: Arc<T>) -> Result<(), Arc<T>> {
        let new_ptr = Arc::<T>::into_raw(arc) as *mut T;
        match self.ptr.compare_exchange(
            null_mut::<T>(),
            new_ptr,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(ptr) => {
                unsafe {
                    Arc::<T>::from_raw(new_ptr);
                }
                let n = unsafe { Arc::from_raw(ptr) };
                let res = n.clone();
                std::mem::forget(n);
                Err(res)
            }
        }
    }

    pub fn swap_existing(&self, old: Arc<T>, new: Arc<T>) -> bool {
        let old_ptr = Arc::<T>::into_raw(old) as *mut T;
        let new_ptr = Arc::<T>::into_raw(new) as *mut T;
        match self.ptr.compare_exchange(
            old_ptr,
            new_ptr,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                true
            }
            Err(_) => {
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                unsafe {
                    Arc::<T>::from_raw(new_ptr);
                }
                false
            }
        }
    }

    pub fn load(&self) -> Option<Arc<T>> {
        let ptr = self.ptr.load(Ordering::Relaxed);
        if ptr == null_mut::<T>() {
            None
        } else {
            let n = unsafe { Arc::from_raw(ptr) };
            let res = n.clone();
            std::mem::forget(n);
            Some(res)
        }
    }

    // Store unconditionally
    pub fn store(&self, arc: Arc<T>) {
        self.ptr.store(Arc::<T>::into_raw(arc) as *mut T, Ordering::Release);
    }

    pub fn clear(&self) {
        unsafe {
            Arc::<T>::from_raw(self.ptr.load(Ordering::Acquire) as *mut T)
        };
        self.ptr.store(null_mut::<T>(), Ordering::Release);
    }

    pub fn clear_existing(&self, old: Arc<T>) -> bool {
        let old_ptr = Arc::<T>::into_raw(old) as *mut T;
        match self.ptr.compare_exchange(
            old_ptr,
            null_mut::<T>(),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                true
            }
            Err(_) => {
                unsafe {
                    Arc::<T>::from_raw(old_ptr);
                }
                false
            }
        }
    }
}

impl<T> Default for APArc<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for APArc<T> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::Relaxed);
        if ptr != null_mut::<T>() {
            unsafe {
                Arc::from_raw(ptr);
            }
        }
    }
}
