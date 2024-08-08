#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::mem::MaybeUninit;
use std::marker::PhantomPinned;
use std::ptr::NonNull;

use const_assert::{Assert, IsTrue};


enum BlockState<const B: usize> {

    FreeLeaf,
    Parent (Box<BlockNode<B>>, Box<BlockNode<B>>),
    AllocatedLeaf

}


struct BlockNode<const B: usize> {

    block_address: NonNull<u8>,
    size: usize,
    state: BlockState<B>

}

impl<const B: usize> BlockNode<B> {

    pub const fn new(size: usize, address: NonNull<u8>) -> Self {
        Self {
            block_address: address,
            size,
            state: BlockState::FreeLeaf
        }
    } 


    /// Assume `alloc_size` <= `block_size`
    fn new_alloc(block_size: usize, address: NonNull<u8>, alloc_size: usize) -> Self {
        Self {
            block_address: address,
            size: block_size,
            state: Self::alloc_down(address, block_size, alloc_size)
        }
    }


    fn alloc_down(block_address: NonNull<u8>, block_size: usize, alloc_size: usize) -> BlockState<B> {

        let half_size = block_size / 2;

        if alloc_size > half_size || block_size == B {
            BlockState::AllocatedLeaf
        } else {
            BlockState::Parent(
                Box::new(BlockNode::new_alloc(half_size, block_address, alloc_size)),
                Box::new(BlockNode::new(half_size, unsafe { NonNull::new_unchecked(block_address.as_ptr().byte_add(half_size)) }))
            )
        }
    }


    pub fn alloc(&mut self, alloc_size: usize) -> Option<NonNull<u8>> {
        
        match &mut self.state {

            BlockState::FreeLeaf => {

                if self.size < alloc_size {
                    None
                } else {

                    self.state = Self::alloc_down(self.block_address, self.size, alloc_size);

                    // Whether it's the whole block or the first child, they share the base address
                    Some(self.block_address)
                }
            },

            BlockState::Parent(a, b) => {
                
                if let Some(ptr) = a.alloc(alloc_size) {
                    Some(ptr)
                } else if let Some(ptr) = b.alloc(alloc_size) {
                    Some(ptr)
                } else {
                    None
                }
            },

            BlockState::AllocatedLeaf => None,
        }
    }


    pub fn free(&mut self, ptr: NonNull<u8>) -> Result<(), AllocError> {
        
        match &mut self.state {

            BlockState::FreeLeaf => Err(AllocError::DoubleFree),

            BlockState::Parent(a, b) => {

                if ptr < b.block_address {
                    a.free(ptr)?;
                } else {
                    b.free(ptr)?;
                }

                if matches!((&a.state, &b.state), (BlockState::FreeLeaf, BlockState::FreeLeaf)) {
                    self.state = BlockState::FreeLeaf;
                }

                Ok(())
            },

            BlockState::AllocatedLeaf => {

                if self.block_address == ptr {
                    self.state = BlockState::FreeLeaf;
                    Ok(())
                } else {
                    Err(AllocError::UnalignedFree)
                }
            },
        }
    }

}


pub enum AllocError {

    /// Not enough memory to perform the requested allocation
    OutOfMemory,
    /// The memory chunk is already free
    DoubleFree,
    /// The pointer is not aligned with any allocated memory block
    UnalignedFree,
    /// The requested allocation size was 0 bytes
    ZeroAllocation,
    /// The freed pointer was null
    NullPtrFree,
    /// The freed pointer was out of the heap bounds
    FreeOutOfBounds

}


/**
    Create a buddy allocator with a heap of `M` bytes and a zero-order block size of `B` bytes.

    A zero-order block is the smallest possible memory block that can be allocated.
    Trying to allocate a memory block smaller than `B` will allocate a block of exactly `B` bytes.
    
    Note that `B` and `M` must be integer powers of 2 such that `M = B * 2^n`, where `n` is a positive integer.
*/
pub struct BuddyAllocator<const M: usize, const B: usize> {
    
    memory: [MaybeUninit<u8>; M],
    alloc_table: BlockNode<B>,
    upper_memory_bound: NonNull<u8>,
    _pin: PhantomPinned

}

impl<const M: usize, const B: usize> BuddyAllocator<M, B> 
where 
    Assert<{ M.is_power_of_two() }>: IsTrue,
    Assert<{ B.is_power_of_two() }>: IsTrue,
    Assert<{ M % B == 0 }>: IsTrue
{

    pub fn new() -> Self {

        let mut res = Self {
            memory: [MaybeUninit::<u8>::uninit(); M],
            #[allow(invalid_value)]
            alloc_table: unsafe { MaybeUninit::uninit().assume_init() },
            upper_memory_bound: NonNull::dangling(),
            _pin: PhantomPinned::default()
        };

        let base_ptr = unsafe { 
            NonNull::new_unchecked(res.memory.as_mut_ptr() as *mut u8)
        };

        res.alloc_table = BlockNode::new(M, base_ptr);

        res.upper_memory_bound = unsafe {
            NonNull::new_unchecked(base_ptr.as_ptr().byte_add(M))
        };
        
        res
    }


    pub fn alloc(&mut self, size: usize) -> Result<NonNull<u8>, AllocError> {
        if size == 0 {
            Err(AllocError::ZeroAllocation)
        } else {
            self.alloc_table.alloc(size).ok_or(AllocError::OutOfMemory)
        }
    }


    pub fn free(&mut self, ptr: *const u8) -> Result<(), AllocError> {

        if let Some(ptr) = NonNull::new(ptr as *mut u8) {

            if ptr >= self.upper_memory_bound {
                Err(AllocError::FreeOutOfBounds)
            } else {
                self.alloc_table.free(ptr)
            }

        } else {
            Err(AllocError::NullPtrFree)
        }
    }

}



#[cfg(test)]
mod tests {

    use std::ptr;

    use super::*;


    #[test]
    fn check_new_allocator() {

        let _alloc = BuddyAllocator::<1024, 8>::new();

    }


    #[test]
    fn check_allocator_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

        assert!(matches!(alloc.alloc(0), Err(AllocError::ZeroAllocation)));

        assert!(matches!(alloc.alloc(1025), Err(AllocError::OutOfMemory)));
    }


    #[test]
    fn check_allocator_within_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

        assert!(alloc.alloc(1).is_ok());
        assert!(alloc.alloc(8).is_ok());
        assert!(alloc.alloc(9).is_ok());
        assert!(alloc.alloc(24).is_ok());
        assert!(alloc.alloc(32).is_ok());
        assert!(alloc.alloc(65).is_ok());
        assert!(alloc.alloc(1000).is_err());
    }


    #[test]
    fn check_free_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

        assert!(matches!(alloc.free(ptr::null()), Err(AllocError::NullPtrFree)));
        assert!(matches!(alloc.free(usize::MAX as *const u8), Err(AllocError::FreeOutOfBounds)));
    }

}


