use buddy_allocator::BuddyAllocator;


#[allow(dead_code)]
fn main() {

    // Create a buddy allocator with a heap size of 1024 bytes and a zero-order block of 8 bytes.
    let mut alloc = BuddyAllocator::<1024, 8>::new(false);

    struct MyStruct (usize, usize, u32);

    // Allocate a memory block that fits an instance of MyStruct.
    let my_ptr = alloc.alloc::<MyStruct>()
        .unwrap_or_else(|alloc_error| panic!("Allocation failed with error {:?}", alloc_error));

    // You can also cast the NonNull<T> to a raw pointer.
    let my_ptr = my_ptr.as_ptr();

    // Initialize the struct.
    unsafe {
        *my_ptr = MyStruct (32, 3, 90);
    }

    // Free the block that contains the struct.
    alloc.free(my_ptr)
        .unwrap_or_else(|alloc_error| panic!("Failed to free pointer {:?} with error {:?}", my_ptr, alloc_error)
    );

}

