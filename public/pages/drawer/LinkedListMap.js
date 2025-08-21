// 100% chatGPT
export class LinkedListMap {
    head = null;
    tail = null;
    map = new Map();
    add(id, value) {
        if (this.map.has(id)) {
            throw new Error(`ID ${id} already exists in the LinkedListMap.`);
        }
        const node = { value, prev: this.tail, next: null };
        if (this.tail) {
            this.tail.next = node;
        }
        else {
            this.head = node; // first node in list
        }
        this.tail = node;
        this.map.set(id, node);
    }
    replace(oldId, newId, newValue) {
        const node = this.map.get(oldId);
        if (!node) {
            throw new Error(`ID ${oldId} can't be replaced; it's not in the LinkedListMap.`);
        }
        if (this.map.has(newId)) {
            throw new Error(`ID ${newId} can't replace; it already exists.`);
        }
        node.value = newValue;
        this.map.delete(oldId);
        this.map.set(newId, node);
    }
    removeById(id) {
        const node = this.map.get(id);
        if (!node)
            return;
        if (node.prev) {
            node.prev.next = node.next;
        }
        else {
            this.head = node.next;
        }
        if (node.next) {
            node.next.prev = node.prev;
        }
        else {
            this.tail = node.prev;
        }
        this.map.delete(id);
    }
    getById(id) {
        const node = this.map.get(id);
        return node ? node.value : null;
    }
    moveToStart(id) {
        const node = this.map.get(id);
        if (!node || node === this.head)
            return;
        if (node.prev)
            node.prev.next = node.next;
        if (node.next)
            node.next.prev = node.prev;
        if (node === this.tail)
            this.tail = node.prev;
        node.prev = null;
        node.next = this.head;
        if (this.head)
            this.head.prev = node;
        this.head = node;
        if (!this.tail)
            this.tail = node;
    }
    moveToEnd(id) {
        const node = this.map.get(id);
        if (!node || node === this.tail)
            return;
        if (node.prev)
            node.prev.next = node.next;
        else
            this.head = node.next;
        if (node.next)
            node.next.prev = node.prev;
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