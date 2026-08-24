// Fase 11 Task 6 (8/N). Estratto da AppTopbar.vue (Task 6 2/N, 6/N) per
// evitare una seconda copia divergente nell'header mobile (§5.2 del
// documento funzionale, tabella `mobileTitleFor()`): stessa mappa,
// stesso identico testo per rotta — la topbar mostra la briciola di
// pane, l'header mobile il titolo, ma per ogni rotta oggi coperta sono
// la stessa identica stringa.
import { ref } from 'vue'

export const ROUTE_TITLE_KEYS: Record<string, string> = {
  '/': 'topbar.allPhotos',
  '/favorites': 'favorites.title',
  '/search': 'nav.cerca',
  '/culling': 'culling.entry',
  '/map': 'maps.entry',
  '/shares': 'shares.entry',
  '/albums': 'albums.entry',
  '/trash': 'trash.entry',
  '/problems': 'problems.title',
  '/batch-edit': 'batchEdit.title',
  '/folders': 'folders.title',
  '/users': 'users.title',
  '/groups': 'groups.title'
}

/** Nome dell'album aperto in `/albums/:id` — Task 12 (1/N). Prima rotta
 * dinamica con un "aperto" osservabile dall'esterno della vista: i debiti
 * dichiarati sopra per `Cartelle / <nome>`/`Culling / <nome lotto>`
 * restano (nessuna di quelle rotte espone ancora uno stato aperto), ma
 * per gli album ora esiste davvero — `AlbumDetailView` lo scrive al
 * caricamento e lo azzera allo smontaggio. Ref di modulo condiviso, non
 * uno store Pinia per un solo campo: stesso principio di `useDensity`
 * (comparso un secondo consumatore, `AppMobileHeader`, subito). */
export const activeAlbumName = ref<string | null>(null)
