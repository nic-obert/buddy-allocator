use buddy_allocator::BuddyAllocator;


#[allow(dead_code)]
fn main() {

    // Create a buddy allocator with a heap size of 1024 bytes and a zero-order block of 8 bytes.
    let mut alloc = BuddyAllocator::<1024, 8>::new(false);

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

