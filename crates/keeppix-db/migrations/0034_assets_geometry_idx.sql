-- Indice di copertura per /timeline/geometry (fase-10 §2.4): la query non
-- porta id né altri campi, solo width/height per riga indicizzata, quindi con
-- INCLUDE si serve dal solo indice (index-only scan) senza toccare la heap.
-- Senza, la stessa query degrada a un seq scan su tutta `assets` (misurato in
-- Task 1bis: EXPLAIN mostrava Seq Scan sia con random_page_cost=4.0 sia 1.1).
CREATE INDEX assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
    INCLUDE (width, height) WHERE status = 'indexed';
