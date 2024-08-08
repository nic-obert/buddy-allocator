#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::mem::{self, MaybeUninit};
use std::marker::PhantomPinned;
use std::ptr::NonNull;

use const_assert::{Assert, IsTrue};


/// The state of an allocation tree node
enum BlockState<const B: usize> {

    FreeLeaf,
    Parent (Box<BlockNode<B>>, Box<BlockNode<B>>),
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

        if alloc_size > half_size || block_size == B {
            (BlockState::AllocatedLeaf, block_size)
        } else {

            let (a, allocated) = BlockNode::new_alloc(half_size, block_address, alloc_size);

            (
                BlockState::Parent(
                    Box::new(a),
                    Box::new(BlockNode::new(half_size, unsafe { NonNull::new_unchecked(block_address.as_ptr().byte_add(half_size)) }))
                ),
                allocated
            )

        }
    }


    /// Recursively try to allocate the requested size.
    pub fn alloc(&mut self, alloc_size: usize) -> Option<(NonNull<u8>, usize)> {
        
        match &mut self.state {

            BlockState::FreeLeaf => {

                if self.size < alloc_size {
                    None
                } else {

                    let (state, allocated) = Self::alloc_down(self.block_address, self.size, alloc_size);
                    self.state = state;

                    // Whether it's the whole block or the first child, they share the base address
                    Some((self.block_address, allocated))
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


    /// Recursively try to free the given pointer.
    pub fn free(&mut self, ptr: NonNull<u8>) -> Result<usize, AllocError> {
        
        match &mut self.state {

            BlockState::FreeLeaf => Err(AllocError::DoubleFree),

            BlockState::Parent(a, b) => {

                let freed = if ptr < b.block_address {
                    a.free(ptr)?
                } else {
                    b.free(ptr)?
                };

                if matches!((&a.state, &b.state), (BlockState::FreeLeaf, BlockState::FreeLeaf)) {
                    self.state = BlockState::FreeLeaf;
                }

                Ok(freed)
            },

            BlockState::AllocatedLeaf => {

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
    pub fn new() -> Self {

        let mut res = Self {
            memory: [MaybeUninit::<u8>::uninit(); M],
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
            Err(AllocError::ZeroAllocation)

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

}



#[cfg(test)]
mod tests {

    use std::ptr;

    use super::*;


    #[test]
    fn check_new_allocator() {

        let alloc = BuddyAllocator::<1024, 8>::new();

        assert_eq!(alloc.total_free(), alloc.heap_size());

    }


    #[test]
    fn check_allocator_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

        assert!(matches!(alloc.alloc_bytes(0), Err(AllocError::ZeroAllocation)));

        assert!(matches!(alloc.alloc_bytes(1025), Err(AllocError::OutOfMemory)));
    }


    #[test]
    fn check_allocator_within_bounds() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

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

        let mut alloc = BuddyAllocator::<1024, 8>::new();

        assert!(matches!(alloc.free(ptr::null() as *const u8), Err(AllocError::NullPtrFree)));
        assert!(matches!(alloc.free(usize::MAX as *const u8), Err(AllocError::FreeOutOfBounds)));
    }


    #[test]
    fn check_full_free() {

        let mut alloc = BuddyAllocator::<1024, 8>::new();

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

