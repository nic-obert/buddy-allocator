# Buddy Allocator

A buddy allocator implemented in Rust.

- [Buddy Allocator](#buddy-allocator)
- [Basic usage](#basic-usage)
- [How it works](#how-it-works)
- [License](#license)

# Basic usage

Allocate and free memory:

```rust
// Create a buddy allocator with a heap size of 1024 bytes and a zero-order block of 8 bytes.
let mut alloc = BuddyAllocator::<1024, 8>::new(false);

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
```

Allocate memory for a structure:

```rust
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
```

Construct the allocator directly on the stack. This approach removes any dependency on the standard system allocator, which is suitable for embedded development or in `#![no_std]` environments where an allocator may not be avalilable.

```rust
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
```

# How it works

This buddy allocator implementation keeps a record of the allocated and free blocks using a binary tree, where each leaf node represents a memory block. Adjacent free nodes are merged to avoid fragmentation and big memory blocks are split in half is the requested allocation is small enough.

A more detailed explanation is available in the source code through comments.


# License

This repository and the code within it are published under the [MIT license](LICENSE).

