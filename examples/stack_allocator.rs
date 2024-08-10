use std::{pin::pin, ptr::NonNull};

use buddy_allocator::BuddyAllocator;


fn main() {

    // Create a buddy allocator with a heap size of 1024 bytes and a zero-order block of 8 bytes.
    // The allocator must be readily pinned because it will live on the stack.
    let mut alloc = pin!( unsafe { 
        BuddyAllocator::<1024, 8>::new_unpinned(false)
    });
    // Initializing a stack-pinned allocator is mandatory to use it safely
    unsafe {
        alloc.as_mut().init_pinned()
    }

    let size_to_alloc: usize = 16;

    // Allocate a memory block.
    let my_pointer: NonNull<u8> = alloc.as_mut().alloc_bytes(size_to_alloc).unwrap_or_else(
        |err| panic!("Allocation failed with error {:?}", err)
    );

    // Do stuff with the pointer...

    // Free the memory block
    alloc.as_mut().free_nonnull(my_pointer).unwrap_or_else(
        |err| panic!("Failed to free pointer {:?} with error {:?}", my_pointer, err)
    ); 


    // Also works with structs
    
    struct MyStruct (usize, usize, u32);

    // Allocate a memory block that fits an instance of MyStruct.
    let my_ptr = alloc.as_mut().alloc::<MyStruct>()
        .unwrap_or_else(|err| panic!("Allocation failed with error {:?}", err));

    // You can also cast the NonNull<T> to a raw pointer.
    let my_ptr = my_ptr.as_ptr();

    // Initialize the struct.
    unsafe {
        *my_ptr = MyStruct (32, 3, 90);
    }

    // Free the block that contains the struct.
    alloc.as_mut().free(my_ptr)
        .unwrap_or_else(|err| panic!("Failed to free pointer {:?} with error {:?}", my_ptr, err)
    );

}

