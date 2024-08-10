use std::ptr::NonNull;
use std::pin::Pin;
use std::mem::{self, MaybeUninit};
use std::marker::PhantomPinned;
use std::cell::UnsafeCell;

use const_assert::{Assert, IsTrue};
use fixed_size_allocator::FixedSizeAllocator;

use crate::{alloc_table::BlockNode, block_node_size, errors::{AllocError, FreeError}};


type ProtoAllocator<const N: usize> = FixedSizeAllocator<{block_node_size!()}, N>;


/**
    Create a buddy allocator with a heap of `M` bytes and a zero-order block size of `B` bytes.

    A zero-order block is the smallest possible memory block that can be allocated.
    Trying to allocate a memory block smaller than `B` will allocate a block of exactly `B` bytes.
    
    Note that `B` and `M` must be integer powers of 2 such that `M = B * 2^n`, where `n` is a positive integer.
*/
pub struct BuddyAllocator<'a, const M: usize, const B: usize>
where 
    [(); M / B]:
{
    
    /// The actual buffer where the heap is stored.
    memory: [MaybeUninit<u8>; M],

    /// A binary  tree that keeps track of the allocated and free blocks.
    alloc_table: BlockNode<'a, B, {M / B}>,

    /// Internal allocator used to allocate the `alloc_table` without relying on external allocators.
    proto_allocator: UnsafeCell<ProtoAllocator<{M / B}>>,
    /// Pin to the proto allocator
    proto_allocator_pin: Pin<&'a mut ProtoAllocator<{M / B}>>,

    /// The highest address of the heap.
    upper_memory_bound: NonNull<u8>,

    /// The total amount of free memory, which may not be available as a whole due to fragmentation.
    total_free: usize,

    /// Tell the compiler this struct should not be moved.
    _pin: PhantomPinned

}

impl<'a, const M: usize, const B: usize> BuddyAllocator<'a, M, B> 
where 
    Assert<{ M.is_power_of_two() }>: IsTrue,
    Assert<{ B.is_power_of_two() }>: IsTrue,
    Assert<{ M % B == 0 }>: IsTrue,
    [(); M / B]:,
{

    // The compiler cannot recognize the type is indeed used
    #[allow(dead_code)]
    type PinnedProtoAllocator = Pin<&'a mut ProtoAllocator<{M / B}>>;


    pub unsafe fn new_unpinned(zero_initialized: bool) -> Self {

        let memory = if zero_initialized {
            [MaybeUninit::<u8>::zeroed(); M]
        } else {
            [MaybeUninit::<u8>::uninit(); M]
        };

        let res = Self {
            memory,
            #[allow(invalid_value)]
            alloc_table: unsafe { MaybeUninit::uninit().assume_init() },
            proto_allocator: UnsafeCell::new(unsafe { FixedSizeAllocator::<{block_node_size!()}, {M / B}>::new_unpinned(false) }),
            proto_allocator_pin: unsafe { Pin::new_unchecked(mem::transmute(NonNull::<Self::PinnedProtoAllocator>::dangling())) },
            upper_memory_bound: NonNull::dangling(),
            total_free: M,
            _pin: PhantomPinned::default()
        };

        res
    }


    pub unsafe fn init_pinned(self: Pin<&mut Self>) {

        let self_data = unsafe { self.get_unchecked_mut() };
        
        // Get the lower bound of the heap
        let base_ptr = unsafe { 
            NonNull::new_unchecked(self_data.memory.as_mut_ptr() as *mut u8)
        };

        // Initialize the allocation table
        self_data.alloc_table = BlockNode::new(M, base_ptr);

        // Calculate the upper bound of the heap
        self_data.upper_memory_bound = unsafe {
            NonNull::new_unchecked(base_ptr.as_ptr().byte_add(M))
        };

        // Store a pin to the proto allocator
        self_data.proto_allocator_pin = unsafe {
            Pin::new_unchecked(self_data.proto_allocator.get().as_mut_unchecked())
        };
    }    


    /// Create a new allocator.
    pub fn new(zero_initialized: bool) -> Pin<Box<Self>> {

        let memory = if zero_initialized {
            [MaybeUninit::<u8>::zeroed(); M]
        } else {
            [MaybeUninit::<u8>::uninit(); M]
        };

        let mut res = Box::new(Self {
            memory,
            #[allow(invalid_value)]
            alloc_table: unsafe { MaybeUninit::uninit().assume_init() },
            proto_allocator: UnsafeCell::new(unsafe { FixedSizeAllocator::<{block_node_size!()}, {M / B}>::new_unpinned(false) }),
            proto_allocator_pin: unsafe { Pin::new_unchecked(mem::transmute(NonNull::<Self::PinnedProtoAllocator>::dangling())) },
            upper_memory_bound: NonNull::dangling(),
            total_free: M,
            _pin: PhantomPinned::default()
        });

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
        
        // Store a pin to the proto allocator
        res.as_mut().proto_allocator_pin = unsafe {
            Pin::new_unchecked(res.proto_allocator.get().as_mut_unchecked())
        };
        
        Box::into_pin(res)
    }


    /// Allocate a memory block big enough to store at least the size of `T`.
    /// Return a pointer to the start of the allocated block.
    /// Pointers allocated throuch this allocator must be freed through this allocator as well.
    pub fn alloc<T>(self: Pin<&mut Self>) -> Result<NonNull<T>, AllocError> {
        unsafe {
            mem::transmute::<Result<NonNull<u8>, AllocError>, Result<NonNull<T>, AllocError>>(
                self.alloc_bytes(mem::size_of::<T>())
            )
        }
    }


    /// Allocate a memory block big enough to store at least `size` bytes.
    /// Return a pointer to the start of the allocated block.
    /// Pointers allocated throuch this allocator must be freed through this allocator as well.
    pub fn alloc_bytes(self: Pin<&mut Self>, size: usize) -> Result<NonNull<u8>, AllocError> {

        let self_mut = unsafe { self.get_unchecked_mut() };

        if size == 0 {
            // Disallow allocating zero bytes.
            // Think: if zero bytes were to be allocated, what is the returned pointer supposed to point to?
            Err(AllocError::ZeroAllocation)

        } else if size > self_mut.total_free() {
            // Cannot ever allocate more than the total free memory
            Err(AllocError::OutOfMemory)
            
        } else if let Some((ptr, allocated)) = self_mut.alloc_table.alloc(size, &mut self_mut.proto_allocator_pin) {
            // Keep track of the free memory
            self_mut.total_free -= allocated;
            Ok(ptr)

        } else {
            Err(AllocError::OutOfMemory)
        }
    }


    /// Free the memory block found at `ptr`.
    /// Note that the block must have been allocated through this allocator.
    pub fn free_nonnull<T>(self: Pin<&mut Self>, ptr: NonNull<T>) -> Result<(), FreeError> {

        let self_data = unsafe { self.get_unchecked_mut() };

        // Drop the generic type. It's irrelevant which type the pointer points to.
        let ptr = unsafe {
            mem::transmute::<NonNull<T>, NonNull<u8>>(ptr)
        };

        if ptr >= self_data.upper_memory_bound || (ptr.as_ptr() as usize) < (self_data.memory.as_ptr() as usize) {
            // Cannot free memory outside of the allocator's heap
            Err(FreeError::FreeOutOfBounds)

        } else {

            match self_data.alloc_table.free(ptr, &mut self_data.proto_allocator_pin) {

                Ok(freed) => {
                    // Keep track of the free memory
                    self_data.total_free += freed;
                    Ok(())
                },

                Err(e) => Err(e)
            }
        }
    }


    /// Free the memory block found at `ptr`.
    /// Note that the block must have been allocated through this allocator.
    pub fn free<T>(self: Pin<&mut Self>, ptr: *const T) -> Result<(), FreeError> {

        if let Some(ptr) = NonNull::new(ptr as *mut u8) {
            self.free_nonnull(ptr)
        } else {
            Err(FreeError::NullPtrFree)
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
        self.proto_allocator_pin.as_mut().free_all();
    }

}

