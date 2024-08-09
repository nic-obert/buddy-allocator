use std::ptr::NonNull;

use buddy_allocator::BuddyAllocator;


fn main() {

    // Create a buddy allocator with a heap size of 1024 bytes and a zero-order block of 8 bytes.
    let mut alloc = BuddyAllocator::<1024, 8>::new(false);

    let size_to_alloc: usize = 16;

    // Allocate a memory block.
    let my_pointer: NonNull<u8> = alloc.alloc_bytes(size_to_alloc).unwrap_or_else(
        |alloc_error| panic!("Allocation failed with error {:?}", alloc_error)
    );

    // Do stuff with the pointer...

    // Free the memory block
    alloc.free_nonnull(my_pointer).unwrap_or_else(
        |alloc_error| panic!("Failed to free pointer {:?} with error {:?}", my_pointer,alloc_error)
    ); 

}

