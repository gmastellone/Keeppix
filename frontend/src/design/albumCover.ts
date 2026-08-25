// §41/§42: la copertina di un album è un gradiente CSS generato, non
// una miniatura reale — `AlbumView.cover_tint`/`monochrome`
// (`crates/keeppix-api/src/routes/albums.rs:37-40`) esistono come
// colonne ma nessuna rotta le scrive mai (`PatchAlbumBody` non ha
// questi campi): sempre assenti/`false` in pratica. Calcolato quindi
// lato client, deterministico sull'id — la stessa scelta già presa per
// `avatarColorFor` (Task 11 1/N), stesso principio "SP-16: hash-based,
// nessuna formula esatta nel documento oltre al vincolo".
export function albumCoverGradient(seed: string): string {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  }
  const hue = hash % 360
  return `linear-gradient(135deg, hsl(${hue}, 40%, 55%), hsl(${(hue + 18) % 360}, 35%, 32%))`
}
