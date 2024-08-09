#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::mem::{self, MaybeUninit};
use std::marker::PhantomPinned;
use std::ptr::NonNull;

use const_assert::{Assert, IsTrue};


/// The state of an allocation tree node.
enum BlockState<const B: usize> {

    /// The node represents a free memory block.
    FreeLeaf,

    // The node represents a memory block that has been split in two buddies.
    Parent { left: Box<BlockNode<B>>, right: Box<BlockNode<B>> },

    // The node represents an already allocated memory block.
    AllocatedLeaf

}


/// Node of the allocation tree.
/// Each node is associated with a memory block.
struct BlockNode<const B: usize> {

    /// Start address of the associated memory block
    block_address: NonNull<u8>,

    /// Size of the associated memory block in bytes.
    size: usize,

    /// State of the associated memory block (free, allocated, split).
    state: BlockState<B>

}

impl<const B: usize> BlockNode<B> {

    /// Create a new free leaf node.
    pub const fn new(size: usize, address: NonNull<u8>) -> Self {
        Self {
            block_address: address,
            size,
            state: BlockState::FreeLeaf
        }
    } 


    /// Create a new node and propagate the allocation.
    /// Assume `alloc_size` <= `block_size`
    fn new_alloc(block_size: usize, address: NonNull<u8>, alloc_size: usize) -> (Self, usize) {
        
        let (state, allocated) =  Self::alloc_down(address, block_size, alloc_size);

        (
            Self {
                block_address: address,
                size: block_size,
                state
            },
            allocated
        )
    }


    /// Recursively propagate the allocation down to the smallest memory block that can fit the requested size.
    fn alloc_down(block_address: NonNull<u8>, block_size: usize, alloc_size: usize) -> (BlockState<B>, usize) {

        let half_size = block_size / 2;

        // If the requested size is greater than half the block size, the block cannot be split.
        // Also, the block cannot be split further if it's a zero-order block.
        if alloc_size > half_size || block_size == B {
            (BlockState::AllocatedLeaf, block_size)

        } else {
            // Split the block in two identical buddy blocks and propagate the allocation.

            let (left, allocated) = BlockNode::new_alloc(half_size, block_address, alloc_size);

            (
                BlockState::Parent {
                    left: Box::new(left),
                    right: Box::new(BlockNode::new(half_size, unsafe { NonNull::new_unchecked(block_address.as_ptr().byte_add(half_size)) }))
                },
                allocated
            )

        }
    }


    /// Recursively try to allocate the requested size.
    pub fn alloc(&mut self, alloc_size: usize) -> Option<(NonNull<u8>, usize)> {
        
        match &mut self.state {

            BlockState::FreeLeaf => {

                if self.size < alloc_size {
                    // The block is too small for the requested size.
                    None

                } else {

                    // If the block is big enough for the requested size, propagate the allocation.
                    let (state, allocated) = Self::alloc_down(self.block_address, self.size, alloc_size);
                    self.state = state;

                    // Whether it's the whole block or the first child, they share the base address
                    Some((self.block_address, allocated))
                }
            },

            BlockState::Parent { left, right } => {

                if self.size <= alloc_size {
                    // The requested allocation will never fit in any of the children since a child is always smaller than a parent.
                    // Stop the search here to avoid useless recursion.
                    None
                }
                // Check if any of the children can allocate the requested memory
                else if let Some(ptr) = left.alloc(alloc_size) {
                    Some(ptr)
                } else if let Some(ptr) = right.alloc(alloc_size) {
                    Some(ptr)
                } else {
                    None
                }
            },

            BlockState::AllocatedLeaf => None,
        }
    }


    /// Recursively try to free the given pointer.
    pub fn free(&mut self, ptr: NonNull<u8>) -> Result<usize, AllocError> {
        
        match &mut self.state {

            // Cannot free a free block.
            BlockState::FreeLeaf => Err(AllocError::DoubleFree),

            BlockState::Parent { left, right } => {

                // Free the node that contains the given pointer.
                let freed = if ptr < right.block_address {
                    left.free(ptr)?
                } else {
                    right.free(ptr)?
                };

                // If both children nodes are free, merge them into a single block to avoid fragmentation.
                if matches!((&left.state, &right.state), (BlockState::FreeLeaf, BlockState::FreeLeaf)) {
                    self.state = BlockState::FreeLeaf;
                }

                Ok(freed)
            },

            BlockState::AllocatedLeaf => {

                // Only allow freeing the block if the given pointer matches the block's start address.
                if self.block_address == ptr {
                    self.state = BlockState::FreeLeaf;
                    Ok(self.size)
                } else {
                    Err(AllocError::UnalignedFree)
                }
            },
        }
    }

}


/// Enum representing errors that may happen during allocation and freeing.
#[derive(Debug, Clone, Copy)]
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
    
    /// The actual buffer where the heap is stored.
    memory: [MaybeUninit<u8>; M],

    /// A binary  tree that keeps track of the allocated and free blocks.
    alloc_table: BlockNode<B>,

    /// The highest address of the heap.
    upper_memory_bound: NonNull<u8>,

    /// The total amount of free memory, which may not be available as a whole due to fragmentation.
    total_free: usize,

    /// Tell the compiler this struct should not be moved.
    _pin: PhantomPinned

}

impl<const M: usize, const B: usize> BuddyAllocator<M, B> 
where 
    Assert<{ M.is_power_of_two() }>: IsTrue,
    Assert<{ B.is_power_of_two() }>: IsTrue,
    Assert<{ M % B == 0 }>: IsTrue
{

    /// Create a new allocator.
    pub fn new(zero_initialized: bool) -> Self {

        let memory = if zero_initialized {
            [MaybeUninit::<u8>::zeroed(); M]
        } else {
            [MaybeUninit::<u8>::uninit(); M]
        };

        let mut res = Self {
            memory,
            #[allow(invalid_value)]
            alloc_table: unsafe { MaybeUninit::uninit().assume_init() },
            upper_memory_bound: NonNull::dangling(),
            total_free: M,
            _pin: PhantomPinned::default()
        };

        // Get the lower bound of the heap
        let base_ptr = unsafe { 
            NonNull::new_unchecked(res.memory.as_mut_ptr() as *mut u8)
        };

        // Initialize the allocation table
        res.alloc_table = BlockNode::new(M, base_ptr);

        // Calculate the upper bound of the heap
        res.upper_memory_bound = unsafe {
            NonNull::new_unchecked(base_ptr.as_ptr().byte_add(M))
        };
        
        res
    }


    /// Allocate a memory block big enough to store at least the size of `T`.
    /// Return a pointer to the start of the allocated block.
    /// Pointers allocated throuch this allocator must be freed through this allocator as well.
    pub fn alloc<T>(&mut self) -> Result<NonNull<T>, AllocError> {
        unsafe {
            mem::transmute::<Result<NonNull<u8>, AllocError>, Result<NonNull<T>, AllocError>>(
                self.alloc_bytes(mem::size_of::<T>())
            )
        }
    }


    /// Allocate a memory block big enough to store at least `size` bytes.
    /// Return a pointer to the start of the allocated block.
    /// Pointers allocated throuch this allocator must be freed through this allocator as well.
    pub fn alloc_bytes(&mut self, size: usize) -> Result<NonNull<u8>, AllocError> {

        if size == 0 {
            // Disallow allocating zero bytes.
            // Think: if zero bytes were to be allocated, what is the returned pointer supposed to point to?
            Err(AllocError::ZeroAllocation)

        } else if size > self.total_free() {
            // Cannot ever allocate more than the total free memory
            Err(AllocError::OutOfMemory)
            
        } else if let Some((ptr, allocated)) = self.alloc_table.alloc(size) {
            // Keep track of the free memory
            self.total_free -= allocated;
            Ok(ptr)

        } else {
            Err(AllocError::OutOfMemory)
        }
    }


    /// Free the memory block found at `ptr`.
    /// Note that the block must have been allocated through this allocator.
    pub fn free_nonnull<T>(&mut self, ptr: NonNull<T>) -> Result<(), AllocError> {

        // Drop the generic type. It's irrelevant which type the pointer points to.
        let ptr = unsafe {
            mem::transmute::<NonNull<T>, NonNull<u8>>(ptr)
        };

        if ptr >= self.upper_memory_bound {
            // Cannot free memory outside of the allocator's heap
            Err(AllocError::FreeOutOfBounds)

        } else {

            match self.alloc_table.free(ptr) {

                Ok(freed) => {
                    // Keep track of the free memory
                    self.total_free += freed;
                    Ok(())
                },

                Err(e) => Err(e)
            }
        }
    }


    /// Free the memory block found at `ptr`.
    /// Note that the block must have been allocated through this allocator.
    pub fn free<T>(&mut self, ptr: *const T) -> Result<(), AllocError> {

        if let Some(ptr) = NonNull::new(ptr as *mut u8) {
            self.free_nonnull(ptr)
        } else {
            Err(AllocError::NullPtrFree)
        }
    }


    /// Return the total amount of free memory in the heap.
    /// Note that this memory may not be usable as a whole because of fragmentation.
    pub const fn total_free(&self) -> usize {
        self.total_free
    }


    /// Return the total size of the allocator's heap.
    pub const fn heap_size(&self) -> usize {
        M
    }


    /// Return the size of allocated memory. That is, the amount of memory that is currently in use.
    pub const fn total_allocated(&self) -> usize {
        self.heap_size() - self.total_free()
    }


    /// Free the entirety of the heap. 
    /// This function is inherently unsafe because it will invalidate all pointers to previously allocated blocks.
    pub unsafe fn free_all(&mut self) {
        self.alloc_table = BlockNode::new(M, self.alloc_table.block_address);
        self.total_free = M;
    }

}



#[cfg(test)]
mod tests {

    use std::ptr;

    use super::*;


    #[test]
    fn check_new_allocator() {

        let alloc = BuddyAllocator::<1024, 8>::new(false);

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }


    #[test]
    fn check_allocator_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(matches!(alloc.alloc_bytes(0), Err(AllocError::ZeroAllocation)));

        assert!(matches!(alloc.alloc_bytes(1025), Err(AllocError::OutOfMemory)));
    }


    #[test]
    fn check_allocator_within_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(alloc.alloc_bytes(1).is_ok());
        assert!(alloc.alloc_bytes(8).is_ok());
        assert!(alloc.alloc_bytes(9).is_ok());
        assert!(alloc.alloc_bytes(24).is_ok());
        assert!(alloc.alloc_bytes(32).is_ok());
        assert!(alloc.alloc_bytes(65).is_ok());
        assert!(alloc.alloc_bytes(1000).is_err());
    }


    #[test]
    fn check_free_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        assert!(matches!(alloc.free(ptr::null() as *const u8), Err(AllocError::NullPtrFree)));
        assert!(matches!(alloc.free(usize::MAX as *const u8), Err(AllocError::FreeOutOfBounds)));
    }


    #[test]
    fn check_full_free() {

        let mut alloc = BuddyAllocator::<1024, 8>::new(false);

        let blocks = [
            1,2,3,4,5,6,7,8,9,32,32,53,12,76,50,21,127
        ];

        let ptrs: Vec<NonNull<u8>> = blocks.iter()
            .map(|&s| alloc.alloc_bytes(s as usize).unwrap())
            .collect();

        for ptr in ptrs {
            assert!(alloc.free_nonnull(ptr).is_ok());
        }

        assert_eq!(alloc.total_free(), alloc.heap_size());
    }   

}

