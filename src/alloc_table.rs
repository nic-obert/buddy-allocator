use std::marker::PhantomData;
use std::ptr::NonNull;
use std::pin::Pin;
use std::mem;

use fixed_size_allocator::FixedSizeAllocator;

use crate::errors::FreeError;
use crate::block_node_size;


/// The state of an allocation tree node.
pub enum BlockState<'proto_alloc, const B: usize, const BLOCK_COUNT: usize> {

    /// The node represents a free memory block.
    FreeLeaf,

    // The node represents a memory block that has been split in two buddies.
    Parent { left: NonNull<BlockNode<'proto_alloc, B, BLOCK_COUNT>>, right: NonNull<BlockNode<'proto_alloc, B, BLOCK_COUNT>> },

    // The node represents an already allocated memory block.
    AllocatedLeaf

}


/// Node of the allocation tree.
/// Each node is associated with a memory block.
pub struct BlockNode<'proto_alloc, const B: usize, const BLOCK_COUNT: usize> {

    /// Start address of the associated memory block
    pub(super) block_address: NonNull<u8>,

    /// Size of the associated memory block in bytes.
    size: usize,

    /// State of the associated memory block (free, allocated, split).
    state: BlockState<'proto_alloc, B, BLOCK_COUNT>,

    _phantom_proto_allocator: PhantomData<Pin<&'proto_alloc mut FixedSizeAllocator<{block_node_size!()}, BLOCK_COUNT>>>

}

impl<'proto_alloc, const B: usize, const BLOCK_COUNT: usize> BlockNode<'proto_alloc, B, BLOCK_COUNT> {

    // The compiler cannot recognize the type is indeed used
    #[allow(dead_code)]
    pub type ProtoAllocator = Pin<&'proto_alloc mut FixedSizeAllocator<{block_node_size!()}, BLOCK_COUNT>>;

    /// Create a new free leaf node.
    pub fn new(size: usize, address: NonNull<u8>) -> Self {
        Self {
            block_address: address,
            size,
            state: BlockState::FreeLeaf,
            _phantom_proto_allocator: Default::default()
        }
    } 


    /// Create a new node and propagate the allocation.
    /// Assume `alloc_size` <= `block_size`
    fn new_alloc(block_size: usize, address: NonNull<u8>, alloc_size: usize, proto_allocator: &mut Self::ProtoAllocator) -> (Self, usize) {
        
        let (state, allocated) =  Self::alloc_down(address, block_size, alloc_size, proto_allocator);

        (
            Self {
                block_address: address,
                size: block_size,
                state,
                _phantom_proto_allocator: Default::default()
            },
            allocated
        )
    }


    /// Recursively propagate the allocation down to the smallest memory block that can fit the requested size.
    fn alloc_down(block_address: NonNull<u8>, block_size: usize, alloc_size: usize, proto_allocator: &mut Self::ProtoAllocator) -> (BlockState<'proto_alloc, B, BLOCK_COUNT>, usize) {

        let half_size = block_size / 2;

        // If the requested size is greater than half the block size, the block cannot be split.
        // Also, the block cannot be split further if it's a zero-order block.
        if alloc_size > half_size || block_size == B {
            (BlockState::AllocatedLeaf, block_size)

        } else {
            // Split the block in two identical buddy blocks and propagate the allocation.

            let (left, allocated) = BlockNode::<B, BLOCK_COUNT>::new_alloc(half_size, block_address, alloc_size, proto_allocator);

            unsafe {

                let left_ptr: NonNull<BlockNode<B, BLOCK_COUNT>> = mem::transmute(proto_allocator.as_mut().alloc_untyped().unwrap());
                left_ptr.write(left);

                let right_ptr: NonNull<BlockNode<B, BLOCK_COUNT>> = mem::transmute(proto_allocator.as_mut().alloc_untyped().unwrap());
                right_ptr.write(
                    BlockNode::new(half_size, NonNull::new_unchecked(block_address.as_ptr().byte_add(half_size)))
                );

                (
                    BlockState::Parent {
                        left: left_ptr,
                        right: right_ptr
                    },
                    allocated
                )
            }
        }
    }


    /// Recursively try to allocate the requested size.
    pub fn alloc(&mut self, alloc_size: usize, proto_allocator: &mut Self::ProtoAllocator) -> Option<(NonNull<u8>, usize)> {
        
        match self.state {

            BlockState::FreeLeaf => {

                if self.size < alloc_size {
                    // The block is too small for the requested size.
                    None

                } else {

                    // If the block is big enough for the requested size, propagate the allocation.
                    let (state, allocated) = Self::alloc_down(self.block_address, self.size, alloc_size, proto_allocator);
                    self.state = state;

                    // Whether it's the whole block or the first child, they share the base address
                    Some((self.block_address, allocated))
                }
            },

            BlockState::Parent { mut left, mut right } => {

                if self.size <= alloc_size {
                    // The requested allocation will never fit in any of the children since a child is always smaller than a parent.
                    // Stop the search here to avoid useless recursion.
                    None
                }
                // Check if any of the children can allocate the requested memory
                else if let Some(ptr) = unsafe { left.as_mut() }.alloc(alloc_size, proto_allocator) {
                    Some(ptr)
                } else if let Some(ptr) = unsafe { right.as_mut() }.alloc(alloc_size, proto_allocator) {
                    Some(ptr)
                } else {
                    None
                }
            },

            BlockState::AllocatedLeaf => None,
        }
    }


    /// Recursively try to free the given pointer.
    pub fn free(&mut self, ptr: NonNull<u8>, proto_allocator: &mut Self::ProtoAllocator) -> Result<usize, FreeError> {
        
        match self.state {

            // Cannot free a free block.
            BlockState::FreeLeaf => Err(FreeError::DoubleFree),

            BlockState::Parent { mut left, mut right } => {

                let left_ref = unsafe { left.as_mut() };
                let right_ref = unsafe { right.as_mut() };

                // Free the node that contains the given pointer.
                let freed = if ptr < right_ref.block_address {
                    left_ref.free(ptr, proto_allocator)?
                } else {
                    right_ref.free(ptr, proto_allocator)?
                };

                // If both children nodes are free, merge them into a single block to avoid fragmentation.
                if matches!((&left_ref.state, &right_ref.state), (BlockState::FreeLeaf, BlockState::FreeLeaf)) {

                    self.state = BlockState::FreeLeaf;

                    // Free the children blockk
                    proto_allocator.as_mut().free_nonnull(left).unwrap();
                    proto_allocator.as_mut().free_nonnull(right).unwrap();
                }

                Ok(freed)
            },

            BlockState::AllocatedLeaf => {

                // Only allow freeing the block if the given pointer matches the block's start address.
                if self.block_address == ptr {
                    self.state = BlockState::FreeLeaf;
                    Ok(self.size)
                } else {
                    Err(FreeError::UnalignedFree)
                }
            },
        }
    }

}


// A crappy workaround to satisfy trait constraints
static_assertions::const_assert_eq!(40, mem::size_of::<BlockNode<0, 0>>());
#[macro_export]
macro_rules! block_node_size {
    () => {
        40
    };
}

