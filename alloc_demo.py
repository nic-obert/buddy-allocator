

class BlockNode:

    def __init__(self, block_addr: int, size: int) -> None:
        self.block = block_addr
        self.size = size
        self.children = []
        self.is_free = True
    

    def alloc(self, size: int) -> int | None:
        
        if not self.is_free:
            return None
        
        if self.has_children():
            
            res = self.children[0].alloc(size)
            if res is None:
                res = self.children[1].alloc(size)
            
            return res

        else:

            if self.size < size:
                # Too large
                return None
            
            if size > self.size/2:
                # Allocate the whole block
                self.is_free = False
                return self.block
            
            # Split the block

            self.children = [
                BlockNode(self.block, self.size/2),
                BlockNode(self.block + self.size/2, self.size/2)
            ]

            return self.children[0].alloc(size)
    

    def free(self, ptr: int) -> bool:

        if self.has_children():

            res = None
            if ptr < self.children[1].block:
                res = self.children[0].free(ptr)
            else:
                res = self.children[1].free(ptr)

            if not res:
                return False

            if self.children[0].is_free and self.children[1].is_free:
                self.children = []
            
            return True

        elif self.block == ptr and not self.is_free:

            self.is_free = True
            return True
        
        else:
            # Either double free or invalid pointer
            return False

    
    def has_children(self) -> bool:
        return len(self.children) != 0


class Allocator:

    def __init__(self, size: int) -> None:

        self.memory = [0] * size

        # max_block_count = size / MIN_BLOCK_SIZE <-- this is to determine how much memory to allocate for the table

        self.table = BlockNode(0, size)


    def malloc(self, size: int) -> int | None:
        return self.table.alloc(size)
        

    def free(self, ptr: int) -> bool:
        return self.table.free(ptr)
        


SIZE = 1024

alloc = Allocator(SIZE)

a = alloc.malloc(40)
print(a)
print(alloc.free(a))


