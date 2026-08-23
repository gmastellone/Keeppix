// Fase 11 Task 6 (8/N). Estratto da AppTopbar.vue (Task 6 2/N, 6/N) per
// evitare una seconda copia divergente nell'header mobile (§5.2 del
// documento funzionale, tabella `mobileTitleFor()`): stessa mappa,
// stesso identico testo per rotta — la topbar mostra la briciola di
// pane, l'header mobile il titolo, ma per ogni rotta oggi coperta sono
// la stessa identica stringa.
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
