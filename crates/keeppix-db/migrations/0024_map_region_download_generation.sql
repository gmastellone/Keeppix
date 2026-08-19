ALTER TABLE map_regions
    ADD COLUMN download_generation uuid;

UPDATE map_regions
   SET download_generation = gen_random_uuid();

ALTER TABLE map_regions
    ALTER COLUMN download_generation SET NOT NULL;

UPDATE jobs j
   SET payload = jsonb_set(
       jsonb_set(
           j.payload,
           '{download_generation}',
           to_jsonb(r.download_generation::text),
           true
       ),
       '{file_path}',
       to_jsonb(r.file_path),
       true
   )
  FROM map_regions r
 WHERE j.kind = 'download_map_region'
   AND j.status IN ('pending', 'running')
   AND j.payload->>'region_id' = r.id;
