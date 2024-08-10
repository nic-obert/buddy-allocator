

/// Enum representing errors that may happen when freeing memory blocks.
#[derive(Debug, Clone, Copy)]
pub enum FreeError {

    /// The memory chunk is already free
    DoubleFree,
    /// The pointer is not aligned with any allocated memory block
    UnalignedFree,
    /// The freed pointer was null
    NullPtrFree,
    /// The freed pointer was out of the heap bounds
    FreeOutOfBounds

}


/// Enum representing errors that may happen when allocating of memory blocks.
#[derive(Debug, Clone, Copy)]
pub enum AllocError {

    /// Not enough memory to perform the requested allocation
    OutOfMemory,
    /// The requested allocation size was 0 bytes
    ZeroAllocation,

}

