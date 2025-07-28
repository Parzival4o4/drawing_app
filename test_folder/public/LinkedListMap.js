// 100% chatGPT
export class LinkedListMap {
    constructor() {
        this.head = null;
        this.tail = null;
        this.map = new Map();
    }
    add(id, value) {
        if (this.map.has(id)) {
            throw new Error(`ID ${id} already exists in the LinkedListMap.`);
        }
        const node = { value, prev: this.tail, next: null };
        // Update tail if the list is not empty
        if (this.tail) {
            this.tail.next = node;
        }
        else {
            // Empty list, this is the first node
            this.head = node;
        }
        this.tail = node;
        this.map.set(id, node);
    }
    replace(oldId, newId, newValue) {
        if (!this.map.has(oldId)) {
            throw new Error(`ID ${oldId} cant be replaced its not in the LinkedListMap.`);
        }
        if (this.map.has(newId)) {
            throw new Error(`ID ${newId} cant replace something it is already present.`);
        }
        const node = this.map.get(oldId);
        node.value = newValue;
        this.map.delete(oldId);
        this.map.set(newId, node);
    }
    removeById(id) {
        const node = this.map.get(id);
        if (!node)
            return;
        // Unlink the node
        if (node.prev) {
            node.prev.next = node.next;
        }
        else {
            // If no previous node, this was the head
            this.head = node.next;
        }
        if (node.next) {
            node.next.prev = node.prev;
        }
        else {
            // If no next node, this was the tail
            this.tail = node.prev;
        }
        this.map.delete(id);
    }
    getById(id) {
        const node = this.map.get(id);
        return node ? node.value : null;
    }
    /** Move the node with this ID to the front (i.e. make it the new head). */
    moveToStart(id) {
        const node = this.map.get(id);
        if (!node || node === this.head)
            return;
        // Unlink node
        if (node.prev)
            node.prev.next = node.next;
        if (node.next)
            node.next.prev = node.prev;
        if (node === this.tail)
            this.tail = node.prev;
        // Insert at head
        node.prev = null;
        node.next = this.head;
        if (this.head)
            this.head.prev = node;
        this.head = node;
        if (!this.tail)
            this.tail = node;
    }
    /** Move the node with this ID to the back (i.e. make it the new tail). */
    moveToEnd(id) {
        const node = this.map.get(id);
        if (!node || node === this.tail)
            return;
        // Unlink node
        if (node.prev)
            node.prev.next = node.next;
        else
            this.head = node.next;
        if (node.next)
            node.next.prev = node.prev;
        // Insert at tail
        node.next = null;
        node.prev = this.tail;
        if (this.tail)
            this.tail.next = node;
        this.tail = node;
        if (!this.head)
            this.head = node;
    }
    [Symbol.iterator]() {
        let current = this.head;
        return {
            next() {
                if (current) {
                    const value = current.value;
                    current = current.next;
                    return { value, done: false };
                }
                return { value: undefined, done: true };
            }
        };
    }
    clear() {
        this.head = null;
        this.tail = null;
        this.map.clear();
    }
    size() {
        return this.map.size;
    }
}
//# sourceMappingURL=LinkedListMap.js.map