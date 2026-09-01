/**
 * LRU cache of loaded pages: at 200,000 shots, the only thing that grows
 * without bound while scrolling the entire library is the cache of
 * already-downloaded assets — geometry (~1.2 MB total) and prefix sums
 * stay small and **are never evicted**, they live outside this class.
 *
 * An explicit cap on the number of resident pages, not on the number of
 * assets: an evicted page reloads with a single request once it's needed
 * again (`TimelineView` requests it again from `IntersectionObserver`),
 * it's not lost forever.
 */
export class LruPageCache<K, V> {
  private readonly capacity: number
  // `Map`'s insertion order is the LRU structure: re-setting an existing
  // key moves it to the end without a separate linked list.
  private readonly map = new Map<K, V>()

  constructor(capacity: number) {
    if (capacity < 1) {
      throw new RangeError('LruPageCache capacity must be at least 1')
    }
    this.capacity = capacity
  }

  get size(): number {
    return this.map.size
  }

  has(key: K): boolean {
    return this.map.has(key)
  }

  /** Read-only iteration for point-patching a cached entry in place
   * (finding which pages contain a specific id) — never mutate the cache
   * while iterating this; collect what needs to change first, then call
   * `set` after the loop ends. */
  entries(): IterableIterator<[K, V]> {
    return this.map.entries()
  }

  get(key: K): V | undefined {
    if (!this.map.has(key)) return undefined
    const value = this.map.get(key) as V
    this.map.delete(key)
    this.map.set(key, value)
    return value
  }

  set(key: K, value: V): void {
    this.map.delete(key)
    this.map.set(key, value)
    if (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value as K
      this.map.delete(oldest)
    }
  }

  delete(key: K): void {
    this.map.delete(key)
  }

  clear(): void {
    this.map.clear()
  }
}
