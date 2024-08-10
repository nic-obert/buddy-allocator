#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(inherent_associated_types)]
#![feature(ptr_as_ref_unchecked)]

mod alloc_table;
mod errors;
mod buddy_allocator;

pub use errors::{AllocError, FreeError};
pub use buddy_allocator::BuddyAllocator;


#[cfg(test)]
mod tests {

    use std::{pin::pin, ptr::{self, NonNull}};

    use buddy_allocator::BuddyAllocator;
    use errors::{AllocError, FreeError};

    use super::*;


    #[test]
    fn check_new_allocator() {

        let alloc = BuddyAllocator::<1024, 8>::new(false);

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }


    #[test]
    fn check_allocator_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(matches!(alloc.as_mut().alloc_bytes(0), Err(AllocError::ZeroAllocation)));

        assert!(matches!(alloc.as_mut().alloc_bytes(1025), Err(AllocError::OutOfMemory)));
    }


    #[test]
    fn check_allocator_within_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(alloc.as_mut().alloc_bytes(1).is_ok());
        assert!(alloc.as_mut().alloc_bytes(8).is_ok());
        assert!(alloc.as_mut().alloc_bytes(9).is_ok());
        assert!(alloc.as_mut().alloc_bytes(24).is_ok());
        assert!(alloc.as_mut().alloc_bytes(32).is_ok());
        assert!(alloc.as_mut().alloc_bytes(65).is_ok());
        assert!(alloc.as_mut().alloc_bytes(1000).is_err());
    }


    #[test]
    fn check_free_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(matches!(alloc.as_mut().free(ptr::null() as *const u8), Err(FreeError::NullPtrFree)));
        assert!(matches!(alloc.as_mut().free(usize::MAX as *const u8), Err(FreeError::FreeOutOfBounds)));
    }


    #[test]
    fn check_full_free() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        let blocks = [
            1,2,3,4,5,6,7,8,9,32,32,53,12,76,50,21,127
        ];

        let ptrs: Vec<NonNull<u8>> = blocks.iter()
            .map(|&s| alloc.as_mut().alloc_bytes(s as usize).unwrap())
            .collect();

        for ptr in ptrs {
            assert!(alloc.as_mut().free_nonnull(ptr).is_ok());
        }

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }   


    #[test]
    fn check_new_allocator_stack() {

        let mut alloc = pin!( unsafe { 
            BuddyAllocator::<1024, 8>::new_unpinned(false)
        });
        unsafe {
            alloc.as_mut().init_pinned()
        }

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }


    #[test]
    fn check_allocator_bounds_stack() {

        let mut alloc = pin!( unsafe { 
            BuddyAllocator::<1024, 8>::new_unpinned(false)
        });
        unsafe {
            alloc.as_mut().init_pinned()
        }

        assert!(matches!(alloc.as_mut().alloc_bytes(0), Err(AllocError::ZeroAllocation)));

        assert!(matches!(alloc.as_mut().alloc_bytes(1025), Err(AllocError::OutOfMemory)));
    }


    #[test]
    fn check_allocator_within_bounds_stack() {

        let mut alloc = pin!( unsafe { 
            BuddyAllocator::<1024, 8>::new_unpinned(false)
        });
        unsafe {
            alloc.as_mut().init_pinned()
        }
        assert!(alloc.as_mut().alloc_bytes(1).is_ok());
        assert!(alloc.as_mut().alloc_bytes(8).is_ok());
        assert!(alloc.as_mut().alloc_bytes(9).is_ok());
        assert!(alloc.as_mut().alloc_bytes(24).is_ok());
        assert!(alloc.as_mut().alloc_bytes(32).is_ok());
        assert!(alloc.as_mut().alloc_bytes(65).is_ok());
        assert!(alloc.as_mut().alloc_bytes(1000).is_err());
    }


    #[test]
    fn check_free_bounds_stack() {

        let mut alloc = pin!( unsafe { 
            BuddyAllocator::<1024, 8>::new_unpinned(false)
        });
        unsafe {
            alloc.as_mut().init_pinned()
        }
        assert!(matches!(alloc.as_mut().free(ptr::null() as *const u8), Err(FreeError::NullPtrFree)));
        assert!(matches!(alloc.as_mut().free(usize::MAX as *const u8), Err(FreeError::FreeOutOfBounds)));
    }


    #[test]
    fn check_full_free_stack() {

        let mut alloc = pin!( unsafe { 
            BuddyAllocator::<1024, 8>::new_unpinned(false)
        });
        unsafe {
            alloc.as_mut().init_pinned()
        }
        let blocks = [
            1,2,3,4,5,6,7,8,9,32,32,53,12,76,50,21,127
        ];

        let ptrs: Vec<NonNull<u8>> = blocks.iter()
            .map(|&s| alloc.as_mut().alloc_bytes(s as usize).unwrap())
            .collect();

        for ptr in ptrs {
            assert!(alloc.as_mut().free_nonnull(ptr).is_ok());
        }

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }   

}

