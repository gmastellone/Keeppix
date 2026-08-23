/**
 * Cache LRU delle pagine caricate (Fase 11 Task 4, piano §4.8): a 200.000
 * scatti l'unica cosa che cresce senza limite scorrendo l'intera libreria
 * è la cache degli asset già scaricati — geometria (~1,2 MB in tutto) e
 * somme prefisse restano piccole e **non si sfrattano mai**, vivono fuori
 * da questa classe.
 *
 * Tetto esplicito sul numero di pagine residenti, non sul numero di
 * asset: una pagina sfrattata si ricarica con una richiesta quando torna
 * a servire (`TimelineView` la richiede di nuovo a `IntersectionObserver`),
 * non è persa per sempre.
 */
export class LruPageCache<K, V> {
  private readonly capacity: number
  // L'ordine di inserimento di `Map` è la struttura LRU: ri-settare una
  // chiave esistente la sposta in fondo senza una lista collegata a parte.
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
}
