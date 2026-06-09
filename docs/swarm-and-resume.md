# Swarm and Resume

ETLE's torrent-like behavior depends on reusable encrypted chunks, progress persistence, and peer availability.

## Chunk Availability

A peer can advertise:

```text
Have { chunks: Vec<u32> }
```

The downloader builds:

```text
chunk_index -> peers that claim to have it
```

This is the availability map.

## Scheduling Goals

A good scheduler should:

- avoid downloading completed chunks
- avoid duplicate in-flight chunks unless retry policy needs it
- prefer peers that have the requested chunk
- tolerate peer failure
- keep workers busy
- write progress only after verification
- keep partial seeders useful without trusting their claims blindly

## Basic Scheduling Algorithm

```text
1. Load completed chunk set.
2. missing = all_chunks - completed.
3. Query peers for Have.
4. Build availability map.
5. Put missing chunks into queue.
6. Spawn parallel workers.
7. Each worker:
   a. pop missing chunk
   b. choose peer
   c. request chunk
   d. verify chunk
   e. persist chunk
   f. mark progress
   g. report event
8. If peer fails:
   a. requeue chunk
   b. mark peer degraded
   c. try another peer
```

## Resume Algorithm

```text
1. Load descriptor.
2. Load progress.
3. Scan chunk files.
4. Re-verify chunk files if necessary.
5. Build completed set.
6. Build missing set.
7. Start normal scheduling for missing set.
```

`--no-resume` asks the daemon to ignore existing reusable chunks for that download job.

## Partial Seeder

A peer is a partial seeder if it has only some verified chunks.

Rules:

- It may advertise only chunks it has.
- It must reject requests for missing chunks.
- It can still help the swarm.
- Discovery may advertise peers that can serve at least useful local share data.
- Download scheduling must rely on Have-based availability, not only peer existence.

## Retry and Backoff

Recommended policy:

```text
on chunk failure:
  increment chunk retry count
  increment peer failure count
  requeue chunk if retry limit not exceeded
  avoid peer temporarily if failure count is high
```

## Correctness Rules

- A chunk is complete only after BLAKE3 ciphertext verification.
- Progress must be updated after the chunk is safely persisted.
- Reconstruct must not begin until all required chunks are complete.
- A peer must not serve chunks it has not verified locally.
- If descriptor and local chunk metadata disagree, fail closed.
- If a local share is deleted, future seeding/downloading must reload library state instead of assuming stale chunks still exist.
