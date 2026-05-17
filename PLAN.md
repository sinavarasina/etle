# ETLE PLAN — Torrent-like Track

## Status Ringkas

ETLE saat ini sudah melewati fondasi MVP single-peer:

- [x] Sprint 1 — Crypto dan Chunking
- [x] Sprint 2 — Protocol dan P2P TCP Transfer
- [x] Sprint 3 — CLI Demo dan Progress Logging
- [ ] Sprint 4 — Descriptor `.etle`, multi-file package, reusable encrypted chunks
- [ ] Sprint 5 — Persistent library state, resume, dan auto-seed setelah download
- [ ] Sprint 6 — Multi-peer dan parallel chunk download
- [ ] Sprint 7 — App Service Abstraction
- [ ] Sprint 8 — GUI

Prioritas sekarang: **lebih torrent-like dulu**. GUI dan AppService tetap penting, tetapi ditunda sampai fungsionalitas torrent-like stabil.

---

## 0. Scope

- [x] Membuat prototipe transfer file/pesan multimedia berbasis P2P
- [x] Menggunakan konsep torrent-like: seeder, peer, chunking, dan distribusi chunk
- [x] Fokus utama pada kriptografi: XChaCha20-Poly1305, BLAKE3, dan X25519
- [x] Tidak menargetkan kompatibilitas penuh dengan BitTorrent standar
- [x] Menyediakan CLI untuk testing dan demo
- [ ] Menyediakan format descriptor `.etle` sebagai metadata share, mirip fungsi `.torrent`
- [ ] Mendukung satu share berisi satu file atau banyak file/folder
- [ ] Mendukung encrypted chunks yang reusable antar peer
- [ ] Mendukung state lokal untuk seed/download/resume
- [ ] Mendukung multi-seeder dan parallel chunk download
- [ ] Menyediakan GUI untuk presentasi/demo visual setelah swarm sederhana stabil

Non-target saat ini:

- [ ] Tidak mengejar BitTorrent compatibility
- [ ] Tidak mengejar public DHT
- [ ] Tidak mengejar NAT traversal penuh
- [ ] Tidak mengejar tracker kompleks
- [ ] Tidak mengejar anonymous routing
- [ ] Tidak mengejar account system

---

## 1. Arsitektur Target

Target arsitektur akhir:

```text
CLI / GUI
   ↓
App Service                # belakangan, setelah torrent-like core stabil
   ↓
Swarm / Scheduler
   ↓
P2P Network Layer
   ↓
Protocol Message Layer
   ↓
Library State + Chunk Store
   ↓
Descriptor / Package Layer
   ↓
Crypto Layer
```

Untuk fase sekarang, CLI boleh tetap langsung memanggil network/state layer agar fungsionalitas selesai dulu:

```text
CLI
 ↓
Network / Swarm
 ↓
Library State
 ↓
Descriptor + Chunk Storage
 ↓
Crypto
```

AppService akan dibuat setelah alur torrent-like stabil, supaya GUI tidak ikut membawa desain yang masih berubah.

---

## 2. Konsep Descriptor `.etle`

ETLE membutuhkan metadata share yang setara dengan `.torrent`, tetapi custom dan tidak kompatibel dengan BitTorrent.

Prinsip desain:

- `descriptor.etle` adalah metadata publik yang boleh dibagikan.
- `secret.etlekey` adalah rahasia lokal yang menyimpan file/share key.
- Satu descriptor bisa mewakili satu file atau satu folder/multi-file package.
- Semua file dalam satu package dipandang sebagai satu logical byte stream.
- Logical stream dipecah menjadi chunk global.
- Chunk terenkripsi harus reusable antar peer agar peer yang sudah download bisa ikut seed.

Struktur descriptor target:

```rust
pub struct EtleDescriptor {
    pub version: u16,
    pub name: String,
    pub share_id: ShareId,
    pub total_size: u64,
    pub chunk_size: u64,
    pub crypto: CryptoSuite,
    pub files: Vec<FileEntry>,
    pub chunks: Vec<ChunkMeta>,
}
```

```rust
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub offset: u64,
    pub blake3_hash: FileId,
}
```

```rust
pub struct ShareId(pub [u8; 32]);
```

`share_id` adalah identitas package/share, bukan hanya hash satu file. Untuk multi-file, `share_id` dihitung dari metadata package yang stabil, misalnya daftar path, size, file hash, chunk size, dan chunk hashes.

---

## 3. Library State Lokal

State lokal memakai satu root `.etle/library/`, bukan memisahkan keras `seeds/` dan `downloads/`. Alasannya: satu share bisa berubah status dari downloading menjadi completed lalu seeding.

Target layout:

```text
.etle/
├── library/
│   └── <share_id>/
│       ├── descriptor.etle      # metadata publik
│       ├── secret.etlekey       # file/share key lokal
│       ├── chunks/
│       │   ├── 000000.etle
│       │   ├── 000001.etle
│       │   └── ...
│       ├── progress.bin         # bitmap/list chunk selesai
│       ├── state.bin            # mode, output dir, peer info dasar
│       └── output/
│           └── ...              # hasil reconstruct untuk download multi-file
│
└── index.bin                    # daftar share lokal
```

Mode share:

```rust
pub enum ShareMode {
    Seeding,
    Downloading,
    Completed,
    Paused,
}
```

State share:

```rust
pub struct ShareState {
    pub share_id: ShareId,
    pub mode: ShareMode,
    pub descriptor_path: PathBuf,
    pub chunks_dir: PathBuf,
    pub output_dir: Option<PathBuf>,
    pub completed_chunks: Vec<u32>,
}
```

---

## 4. Model Kripto Target

Model sekarang masih session-derived file key:

```text
X25519 shared_secret + file_id → file_key → encrypt file khusus untuk koneksi itu
```

Model ini bekerja untuk single peer, tetapi tidak cocok untuk swarm karena ciphertext bisa berbeda untuk tiap peer.

Target torrent-like:

```text
random file_key dibuat sekali per share
file_key → encrypt chunks reusable
X25519 shared_secret → session_key
session_key → wrap/unwrap file_key saat peer authorized connect
```

Flow target:

```text
Seeder:
1. create descriptor dari file/folder
2. generate random file_key
3. encrypt package menjadi reusable encrypted chunks
4. simpan descriptor.etle, secret.etlekey, chunks
5. saat peer connect:
   - Hello
   - X25519 key exchange
   - derive session_key
   - kirim descriptor/manifest
   - kirim WrappedFileKey
   - layani RequestChunk

Peer:
1. connect ke seeder
2. Hello
3. X25519 key exchange
4. terima descriptor/manifest
5. terima WrappedFileKey
6. unwrap file_key
7. request chunks
8. verify BLAKE3 encrypted chunk
9. simpan encrypted chunk ke library
10. decrypt/reconstruct output
11. setelah complete, bisa seed dari state yang sama
```

---

## 5. Struktur Direktori Source Target

```text
etle/
├── Cargo.toml
├── PLAN.md
├── src/
│   ├── lib.rs
│   ├── crypto/
│   │   ├── mod.rs
│   │   ├── aead.rs
│   │   ├── hash.rs
│   │   ├── key_exchange.rs
│   │   └── key_wrap.rs
│   ├── file/
│   │   ├── mod.rs
│   │   ├── chunker.rs
│   │   ├── manifest.rs          # legacy/single-file manifest, bisa digabung bertahap
│   │   ├── descriptor.rs        # EtleDescriptor, ShareId, FileEntry
│   │   ├── package.rs           # multi-file logical stream
│   │   └── storage.rs
│   ├── state/
│   │   ├── mod.rs
│   │   ├── library.rs
│   │   ├── progress.rs
│   │   └── error.rs
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── message.rs
│   │   ├── codec.rs
│   │   └── error.rs
│   ├── network/
│   │   ├── mod.rs
│   │   ├── tcp.rs
│   │   ├── handshake.rs
│   │   ├── key_exchange.rs
│   │   ├── transfer.rs
│   │   └── swarm.rs
│   ├── app/
│   │   ├── mod.rs
│   │   ├── command.rs
│   │   ├── event.rs
│   │   └── service.rs
│   ├── gui/
│   │   └── mod.rs
│   └── bin/
│       ├── etle-cli.rs
│       └── etle-gui.rs
├── tests/
│   ├── crypto_test.rs
│   ├── chunk_test.rs
│   ├── descriptor_test.rs
│   ├── state_test.rs
│   ├── protocol_test.rs
│   ├── network_test.rs
│   ├── swarm_test.rs
│   └── transfer_test.rs
└── examples/
    ├── local_roundtrip.rs
    └── debug_roundtrip.rs
```

---

# Task List

## Phase 0 — Project Setup

### T00 — Inisialisasi Project Rust

- [x] Membuat project Rust baru
- [x] Menentukan nama crate
- [x] Menggunakan Rust edition terbaru yang stabil di environment tim
- [x] Memastikan `cargo build` berhasil

Definition of Done:

- [x] `cargo build` sukses
- [x] `cargo test` sukses

---

### T01 — Setup Module Structure

- [x] Membuat `src/lib.rs`
- [x] Membuat folder `crypto/`
- [x] Membuat folder `file/`
- [x] Membuat folder `protocol/`
- [x] Membuat folder `network/`
- [x] Membuat folder `app/`
- [x] Membuat folder `gui/`
- [x] Membuat folder `src/bin/`

Definition of Done:

- [x] Semua module bisa di-import dari `lib.rs`
- [x] `cargo check` sukses

---

### T02 — Setup Cargo Features dan Multi Binary

- [x] Menambahkan binary `etle-cli`
- [x] Menambahkan binary `etle-gui`
- [x] Membuat feature `cli`
- [x] Membuat feature `gui-relm4`
- [x] Membuat GUI dependency optional

Definition of Done:

- [x] `cargo run --bin etle-cli -- --help` bisa dijalankan
- [x] GUI binary placeholder bisa dibuild minimal sesuai feature yang tersedia

---

## Phase 1 — Crypto Core

### T03 — Implement BLAKE3 Hashing

- [x] Membuat fungsi hash untuk bytes
- [x] Membuat fungsi hash untuk file
- [x] Membuat fungsi hash untuk chunk
- [x] Membuat tipe `FileId`
- [x] Membuat tipe `ChunkHash`

Definition of Done:

- [x] Hash input yang sama menghasilkan output yang sama
- [x] Hash input berbeda menghasilkan output berbeda
- [x] Unit test hashing sukses

---

### T04 — Implement XChaCha20-Poly1305 AEAD

- [x] Membuat fungsi encrypt chunk
- [x] Membuat fungsi decrypt chunk
- [x] Membuat generator nonce 24-byte
- [x] Memastikan authentication tag ikut dalam ciphertext

Definition of Done:

- [x] Plaintext bisa dienkripsi dan didekripsi kembali
- [x] Ciphertext yang diubah gagal decrypt
- [x] Nonce salah gagal decrypt

---

### T05 — Implement X25519 Key Exchange

- [x] Membuat ephemeral keypair
- [x] Mengekspor public key
- [x] Menghitung shared secret
- [x] Memastikan dua peer menghasilkan shared secret yang sama

Definition of Done:

- [x] Peer A dan Peer B menghasilkan shared secret identik
- [x] Unit test key exchange sukses

---

### T06 — Implement Key Derivation

- [x] Membuat fungsi derivasi key dari shared secret dan file/share id
- [x] Output derivasi berupa 32-byte key

Definition of Done:

- [x] Shared secret dan ID yang sama menghasilkan key yang sama
- [x] ID berbeda menghasilkan key berbeda

---

### T07 — Implement AAD Format

- [x] Membuat format AAD untuk AEAD
- [x] AAD memuat file/share id
- [x] AAD memuat `chunk_index`
- [x] AAD memuat `chunk_size`
- [x] AAD dipakai saat encrypt dan decrypt

Definition of Done:

- [x] Chunk dengan AAD benar berhasil decrypt
- [x] Chunk dengan `chunk_index` salah gagal decrypt
- [x] Chunk dengan file/share id salah gagal decrypt

---

### T08 — Crypto Unit Tests

- [x] Test BLAKE3 hash
- [x] Test AEAD encrypt/decrypt
- [x] Test tampered ciphertext
- [x] Test wrong nonce
- [x] Test wrong AAD
- [x] Test X25519 shared secret
- [x] Test key derivation

Definition of Done:

- [x] Semua test crypto sukses

---

## Phase 2 — File Chunking dan Manifest

### T09 — Implement File Chunker

- [x] Membaca file dari disk
- [x] Memecah file menjadi chunk
- [x] Mendukung chunk size configurable
- [x] Menjaga urutan chunk berdasarkan index

Definition of Done:

- [x] File kecil bisa dipecah menjadi beberapa chunk
- [x] File besar bisa dipecah menjadi banyak chunk
- [x] Chunk terakhir boleh lebih kecil dari chunk size

---

### T10 — Implement Chunk Metadata

- [x] Membuat struct `ChunkMeta`
- [x] Menyimpan `index`
- [x] Menyimpan `plain_size`
- [x] Menyimpan `encrypted_size`
- [x] Menyimpan `nonce`
- [x] Menyimpan `blake3_hash`

Definition of Done:

- [x] Metadata terbentuk untuk setiap chunk
- [x] Metadata sesuai dengan chunk yang dibuat

---

### T11 — Implement Manifest Structure

- [x] Membuat struct `Manifest`
- [x] Menyimpan `file_id`
- [x] Menyimpan `file_name`
- [x] Menyimpan `file_size`
- [x] Menyimpan `chunk_size`
- [x] Menyimpan list `ChunkMeta`

Definition of Done:

- [x] Manifest bisa dibuat dari file input
- [x] Manifest memiliki jumlah chunk yang benar

---

### T12 — Manifest Serialization

- [x] Serialize manifest ke binary
- [x] Deserialize manifest dari binary
- [x] Menambahkan test roundtrip serialization

Definition of Done:

- [x] Manifest sebelum serialize sama dengan manifest setelah deserialize

---

### T13 — Implement Encrypted Chunk Storage

- [x] Menyimpan encrypted chunk ke storage sementara
- [x] Membaca encrypted chunk berdasarkan index
- [x] Menyimpan nonce dan metadata di manifest
- [x] Memastikan plaintext tidak perlu disimpan setelah encrypt

Definition of Done:

- [x] Encrypted chunk bisa disimpan
- [x] Encrypted chunk bisa dibaca ulang
- [x] Metadata chunk tetap valid

---

### T14 — Local Reconstruct Test

- [x] File dipecah menjadi chunk
- [x] Chunk dienkripsi
- [x] Chunk diverifikasi
- [x] Chunk didekripsi
- [x] File disusun ulang
- [x] Hash file hasil dibandingkan dengan hash file awal

Definition of Done:

- [x] `BLAKE3(input_file) == BLAKE3(output_file)`

---

## Phase 3 — Protocol Message

### T15 — Define Wire Message

- [x] Membuat enum `WireMessage`
- [x] Menambahkan message `Hello`
- [x] Menambahkan message `KeyExchange`
- [x] Menambahkan message `RequestManifest`
- [x] Menambahkan message `Manifest`
- [x] Menambahkan message `Have`
- [x] Menambahkan message `RequestChunk`
- [x] Menambahkan message `Chunk`
- [x] Menambahkan message `Error`

Definition of Done:

- [x] Semua message bisa di-serialize
- [x] Semua message bisa di-deserialize

---

### T16 — Implement Protocol Codec

- [x] Membuat framing message
- [x] Menambahkan length-prefix encoding
- [x] Membuat fungsi send message
- [x] Membuat fungsi receive message
- [x] Menangani error decode
- [x] Menolak frame kosong
- [x] Menolak frame terlalu besar
- [x] Menolak trailing bytes

Definition of Done:

- [x] Message bisa dikirim lewat stream lokal
- [x] Message yang diterima sama dengan message yang dikirim

---

### T17 — Protocol Serialization Tests

- [x] Test wire message serialization roundtrip
- [x] Test codec send/receive
- [x] Test empty frame rejection
- [x] Test oversized frame rejection
- [x] Test invalid/trailing message handling

Definition of Done:

- [x] Semua protocol test sukses

---

## Phase 4 — Basic P2P Network

### T18 — Implement TCP Listener

- [x] Membuat seeder listener
- [x] Menerima koneksi peer
- [x] Logging koneksi masuk via CLI/progress layer

Definition of Done:

- [x] Seeder bisa listen di address tertentu

---

### T19 — Implement TCP Client Connect

- [x] Membuat peer client
- [x] Connect ke seeder address
- [x] Mengirim message awal
- [x] Logging status koneksi via CLI/progress layer

Definition of Done:

- [x] Peer bisa connect ke seeder lokal

---

### T20 — Implement Hello Handshake

- [x] Peer mengirim `Hello`
- [x] Seeder membalas `Hello`
- [x] Menyimpan/menampilkan `peer_id`
- [x] Menolak koneksi invalid

Definition of Done:

- [x] Seeder dan peer saling mengetahui `peer_id`

---

### T21 — Implement Network Key Exchange

- [x] Peer membuat ephemeral X25519 keypair
- [x] Seeder membuat ephemeral X25519 keypair
- [x] Keduanya bertukar public key via `KeyExchange`
- [x] Keduanya derive key yang sama untuk sesi single-peer saat ini
- [ ] Refactor menjadi session key untuk wrapping file/share key

Definition of Done:

- [x] Seeder dan peer memiliki key yang sama
- [x] Key tidak dikirim langsung melalui network

---

### T22 — Test Two Peer Connection

- [x] Jalankan seeder lokal
- [x] Jalankan peer lokal
- [x] Peer connect ke seeder
- [x] Hello handshake sukses
- [x] Key exchange sukses

Definition of Done:

- [x] Test koneksi dan key exchange berhasil

---

## Phase 5 — Seeder dan Peer Transfer

### T23 — Seeder Load File and Manifest

- [x] Seeder menerima path file
- [x] Seeder melakukan chunking
- [x] Seeder melakukan hashing
- [x] Seeder melakukan encryption
- [x] Seeder membuat manifest
- [x] Seeder siap melayani request manifest dan chunk

Definition of Done:

- [x] Seeder siap dengan encrypted chunks dan manifest

---

### T24 — Peer Request/Receive Manifest

- [x] Peer menerima manifest dari seeder
- [x] Peer menyimpan manifest selama transfer
- [x] Peer menampilkan info file

Definition of Done:

- [x] Peer menerima manifest dengan benar

---

### T25 — Peer Request Chunk

- [x] Peer membaca daftar chunk dari manifest
- [x] Peer mengirim `RequestChunk`
- [x] Peer meminta chunk berdasarkan index

Definition of Done:

- [x] Seeder menerima request chunk yang valid

---

### T26 — Seeder Send Encrypted Chunk

- [x] Seeder mencari encrypted chunk berdasarkan index
- [x] Seeder mengirim message `Chunk`
- [x] Seeder menangani request invalid

Definition of Done:

- [x] Peer menerima encrypted chunk dari seeder

---

### T27 — Peer Verify BLAKE3

- [x] Peer menghitung hash chunk yang diterima
- [x] Peer membandingkan dengan hash di manifest
- [x] Peer menolak chunk yang hash-nya tidak cocok

Definition of Done:

- [x] Chunk valid diterima
- [x] Chunk rusak ditolak

---

### T28 — Peer Decrypt Chunk

- [x] Peer membuat AAD berdasarkan manifest
- [x] Peer decrypt chunk dengan XChaCha20-Poly1305
- [x] Peer menolak chunk jika AEAD tag invalid

Definition of Done:

- [x] Chunk valid bisa didekripsi
- [x] Chunk dengan AAD salah gagal decrypt

---

### T29 — Peer Reconstruct Output File

- [x] Peer menyimpan encrypted chunk berdasarkan index selama transfer
- [x] Peer menulis ulang file sesuai urutan chunk
- [x] Peer menangani chunk terakhir yang ukurannya lebih kecil

Definition of Done:

- [x] File output berhasil dibuat

---

### T30 — Verify Final File Hash

- [x] Peer menghitung BLAKE3 file output
- [x] Peer membandingkan dengan `file_id`
- [x] Peer menandai transfer complete jika hash cocok

Definition of Done:

- [x] `BLAKE3(input_file) == BLAKE3(output_file)`

---

## Phase 6 — CLI App

### T31 — CLI Command `seed`

- [x] Menambahkan command `seed`
- [x] Parameter file path
- [x] Parameter listen address
- [x] Parameter chunk size
- [x] Menjalankan seeder dari terminal

Definition of Done:

- [x] `etle-cli seed ./sample.mp4 --listen 127.0.0.1:7000` berjalan

---

### T32 — CLI Command `connect`

- [x] Menambahkan command `connect`
- [x] Parameter peer address
- [x] Melakukan koneksi ke seeder
- [x] Melakukan hello handshake

Definition of Done:

- [x] `etle-cli connect --peer 127.0.0.1:7000` berjalan untuk probe handshake

---

### T33 — CLI Command `download`

- [x] Menambahkan command `download`
- [x] Parameter peer address
- [x] Parameter output path
- [x] Receive manifest
- [x] Download semua chunk
- [x] Reconstruct file
- [x] Verify final hash

Definition of Done:

- [x] File bisa didownload dari seeder melalui CLI

---

### T34 — CLI Progress Logging

- [x] Menampilkan koneksi peer
- [x] Menampilkan status key exchange
- [x] Menampilkan progress chunk
- [x] Menampilkan status BLAKE3 verification
- [x] Menampilkan status reconstruct/final hash
- [x] Menambahkan `--verbose`

Definition of Done:

- [x] Progress transfer terlihat jelas di terminal

---

### T35 — CLI End-to-End Demo

- [x] Menjalankan seeder dari terminal pertama
- [x] Menjalankan peer dari terminal kedua
- [x] Peer mendownload file
- [x] File output sama dengan file input
- [x] Log menunjukkan crypto verification sukses

Definition of Done:

- [x] Demo CLI berhasil dari awal sampai akhir

---

# Torrent-like Core

## Phase 7 — Descriptor dan Multi-file Package

### T36 — Define ShareId

- [ ] Membuat tipe `ShareId`
- [ ] Implement `Display` hex
- [ ] Implement serialize/deserialize
- [ ] Menentukan cara menghitung `share_id` dari descriptor/package metadata

Dependency:

- [x] T03
- [x] T12

Definition of Done:

- [ ] `ShareId` stabil untuk package yang sama
- [ ] `ShareId` berbeda jika isi package berubah

---

### T37 — Define EtleDescriptor

- [ ] Membuat `EtleDescriptor`
- [ ] Menyimpan version
- [ ] Menyimpan name
- [ ] Menyimpan share_id
- [ ] Menyimpan total_size
- [ ] Menyimpan chunk_size
- [ ] Menyimpan crypto suite
- [ ] Menyimpan files
- [ ] Menyimpan chunks

Dependency:

- [ ] T36
- [x] T10

Definition of Done:

- [ ] Descriptor cukup untuk reconstruct file/folder
- [ ] Descriptor tidak menyimpan file_key rahasia

---

### T38 — Define FileEntry dan Logical Stream

- [ ] Membuat `FileEntry`
- [ ] Menyimpan relative path
- [ ] Menyimpan size
- [ ] Menyimpan offset global
- [ ] Menyimpan BLAKE3 hash per file
- [ ] Membuat representasi logical byte stream untuk multi-file package

Dependency:

- [ ] T37

Definition of Done:

- [ ] Satu folder bisa dipetakan menjadi list file deterministic
- [ ] Offset file valid dan tidak overlap

---

### T39 — Descriptor Serialization

- [ ] Serialize descriptor ke `descriptor.etle`
- [ ] Deserialize descriptor dari `descriptor.etle`
- [ ] Menolak descriptor versi tidak didukung
- [ ] Test roundtrip descriptor

Dependency:

- [ ] T37
- [ ] T38

Definition of Done:

- [ ] Descriptor sebelum serialize sama dengan setelah deserialize

---

### T40 — Package Builder untuk File dan Folder

- [ ] Membuat builder dari single file
- [ ] Membuat builder dari folder
- [ ] Sort path secara deterministic
- [ ] Mengabaikan file internal `.etle` jika input berupa folder
- [ ] Menghasilkan descriptor awal dan plain chunk stream

Dependency:

- [ ] T38
- [x] T09

Definition of Done:

- [ ] Single file dan folder bisa menjadi satu package descriptor

---

### T41 — Package Reconstruction

- [ ] Reconstruct single file dari logical stream
- [ ] Reconstruct folder/multi-file dari logical stream
- [ ] Membuat folder output otomatis
- [ ] Verify hash per file setelah reconstruct

Dependency:

- [ ] T40
- [x] T14

Definition of Done:

- [ ] Folder hasil reconstruct identik secara content dengan folder input

---

### T42 — Descriptor CLI Create Command

- [ ] Menambahkan `etle-cli create <INPUT> --output <NAME>.etle`
- [ ] Mendukung input file
- [ ] Mendukung input folder
- [ ] Menampilkan share_id
- [ ] Menampilkan jumlah file, total size, jumlah chunk

Dependency:

- [ ] T39
- [ ] T40

Definition of Done:

- [ ] Descriptor `.etle` bisa dibuat dari CLI

---

### T43 — Descriptor Tests

- [ ] Test single file descriptor
- [ ] Test multi-file descriptor
- [ ] Test deterministic file ordering
- [ ] Test descriptor roundtrip
- [ ] Test descriptor rejects invalid version/corrupt bytes

Dependency:

- [ ] T39
- [ ] T40

Definition of Done:

- [ ] Semua descriptor/package test sukses

---

## Phase 8 — Reusable File Key dan Key Wrapping

### T44 — Generate Reusable File Key

- [ ] Membuat `generate_file_key() -> SymmetricKey`
- [ ] File key random 32-byte
- [ ] File key dibuat sekali per share/package
- [ ] File key tidak masuk descriptor publik

Dependency:

- [x] T04

Definition of Done:

- [ ] Dua panggilan generate menghasilkan key berbeda

---

### T45 — Define EtleSecret

- [ ] Membuat `EtleSecret`
- [ ] Menyimpan share_id
- [ ] Menyimpan file_key
- [ ] Serialize ke `secret.etlekey`
- [ ] Deserialize dari `secret.etlekey`

Dependency:

- [ ] T36
- [ ] T44

Definition of Done:

- [ ] Secret bisa disimpan dan dibaca ulang
- [ ] Secret cocok dengan share_id descriptor

---

### T46 — Derive Session Key for Wrapping

- [ ] Refactor derivasi dari X25519 menjadi `session_key`
- [ ] Context derivation berbeda dari file encryption key
- [ ] Input memakai shared secret dan share_id
- [ ] Output berupa `SymmetricKey`

Dependency:

- [x] T05
- [ ] T36

Definition of Done:

- [ ] Seeder dan peer menghasilkan session_key yang sama
- [ ] Share berbeda menghasilkan session_key berbeda

---

### T47 — Implement Wrapped File Key

- [ ] Membuat struct `WrappedFileKey`
- [ ] Menyimpan nonce
- [ ] Menyimpan encrypted file_key bytes
- [ ] Membuat `wrap_file_key(session_key, file_key, share_id)`
- [ ] Membuat `unwrap_file_key(session_key, wrapped, share_id)`
- [ ] AAD memakai share_id dan context key wrap

Dependency:

- [x] T04
- [ ] T45
- [ ] T46

Definition of Done:

- [ ] File key bisa dibungkus dan dibuka kembali
- [ ] Wrapped key gagal dibuka jika session_key/share_id salah

---

### T48 — Protocol Message `WrappedFileKey`

- [ ] Menambahkan `WireMessage::WrappedFileKey`
- [ ] Serialize/deserialize message baru
- [ ] Menambahkan test protocol roundtrip

Dependency:

- [x] T15
- [ ] T47

Definition of Done:

- [ ] Wrapped key bisa dikirim lewat protocol codec

---

### T49 — Refactor Transfer to Reusable Encrypted Chunks

- [ ] Seeder encrypt package memakai file_key random, bukan session-derived key
- [ ] Seeder mengirim descriptor/manifest
- [ ] Seeder mengirim wrapped file_key setelah X25519
- [ ] Peer unwrap file_key
- [ ] Peer download encrypted chunks yang reusable
- [ ] Peer decrypt/reconstruct dengan file_key

Dependency:

- [ ] T47
- [ ] T48
- [x] T30

Definition of Done:

- [ ] Single-peer transfer tetap berhasil
- [ ] Ciphertext chunk tidak bergantung pada peer/session

---

### T50 — Reusable Ciphertext Tests

- [ ] Test dua peer menerima chunk ciphertext yang sama untuk share yang sama
- [ ] Test peer bisa decrypt dengan unwrapped file_key
- [ ] Test wrapped key salah ditolak
- [ ] Test final file hash tetap cocok

Dependency:

- [ ] T49

Definition of Done:

- [ ] Semua test reusable file key sukses

---

## Phase 9 — Persistent Library State dan Resume

### T51 — Define Library Paths

- [ ] Membuat layout `.etle/library/<share_id>/`
- [ ] Membuat path helper untuk `descriptor.etle`
- [ ] Membuat path helper untuk `secret.etlekey`
- [ ] Membuat path helper untuk `chunks/`
- [ ] Membuat path helper untuk `progress.bin`
- [ ] Membuat path helper untuk `state.bin`

Dependency:

- [ ] T36

Definition of Done:

- [ ] Semua path state bisa dihasilkan secara deterministic

---

### T52 — Define ShareState dan ShareMode

- [ ] Membuat `ShareMode`
- [ ] Membuat `ShareState`
- [ ] Serialize/deserialize state
- [ ] Menyimpan output_dir untuk download
- [ ] Menyimpan completed chunks

Dependency:

- [ ] T51

Definition of Done:

- [ ] State bisa disimpan dan dibaca ulang

---

### T53 — Implement Progress Bitmap/List

- [ ] Membuat progress chunk tracking
- [ ] Mark chunk complete setelah BLAKE3 verify
- [ ] Load progress saat resume
- [ ] Hitung missing chunks

Dependency:

- [ ] T52

Definition of Done:

- [ ] Progress tetap ada setelah proses dimatikan dan dijalankan ulang

---

### T54 — Persist Seed State

- [ ] `create` atau `seed` menyimpan descriptor
- [ ] Menyimpan secret file key
- [ ] Menyimpan encrypted chunks
- [ ] Menandai mode `Seeding`

Dependency:

- [ ] T45
- [ ] T49
- [ ] T52

Definition of Done:

- [ ] Seeder bisa dimulai ulang dari library state tanpa re-encrypt input original

---

### T55 — Persist Download State

- [ ] Download menyimpan descriptor
- [ ] Download menyimpan secret file key setelah unwrap
- [ ] Download menyimpan encrypted chunk yang sudah verified
- [ ] Download menyimpan progress
- [ ] Download bisa resume missing chunks

Dependency:

- [ ] T52
- [ ] T53
- [ ] T49

Definition of Done:

- [ ] Download bisa dilanjutkan setelah dihentikan

---

### T56 — Seed From State

- [ ] Menambahkan command `seed-state <SHARE_ID>` atau `seed <descriptor.etle> --from-library`
- [ ] Seeder membaca descriptor, secret, dan chunks dari `.etle/library`
- [ ] Seeder tidak membutuhkan file original jika chunks lengkap

Dependency:

- [ ] T54

Definition of Done:

- [ ] Share bisa di-seed hanya dari library state

---

### T57 — Auto-seed After Download

- [ ] Setelah download complete, mode berubah menjadi `Completed` atau `Seeding`
- [ ] Peer yang sudah selesai bisa melayani RequestChunk
- [ ] CLI menyediakan opsi `--seed-after-download`

Dependency:

- [ ] T55
- [ ] T56

Definition of Done:

- [ ] Peer hasil download bisa menjadi seeder untuk peer lain

---

### T58 — State Tests

- [ ] Test seed state roundtrip
- [ ] Test download progress save/load
- [ ] Test resume missing chunk list
- [ ] Test seed from completed download state

Dependency:

- [ ] T54
- [ ] T55
- [ ] T57

Definition of Done:

- [ ] Semua state tests sukses

---

## Phase 10 — Multi-peer Sequential dan Fallback

### T59 — CLI Multiple Peers

- [ ] `download` menerima banyak `--peer`
- [ ] Validasi minimal satu peer
- [ ] Menampilkan daftar peer target

Dependency:

- [ ] T55

Definition of Done:

- [ ] CLI bisa menerima lebih dari satu peer address

---

### T60 — Peer Session Metadata

- [ ] Menyimpan peer address
- [ ] Menyimpan peer_id remote
- [ ] Menyimpan connection status
- [ ] Menyimpan chunk availability dasar

Dependency:

- [ ] T59

Definition of Done:

- [ ] Downloader mengetahui peer mana aktif

---

### T61 — Implement Have Query/Response

- [ ] Peer mengirim `Have { chunks }`
- [ ] Seeder/peer menjawab daftar chunk yang dimiliki
- [ ] Partial seeder bisa mengirim chunk list sebagian

Dependency:

- [x] T15
- [ ] T55

Definition of Done:

- [ ] Downloader tahu availability chunk per peer

---

### T62 — Chunk Availability Map

- [ ] Membuat map `chunk_index -> peer list`
- [ ] Update map dari message `Have`
- [ ] Fallback jika peer tidak punya chunk

Dependency:

- [ ] T61

Definition of Done:

- [ ] Sistem tahu peer mana punya chunk tertentu

---

### T63 — Multi-peer Sequential Download

- [ ] Download chunk dari peer berbeda secara sequential
- [ ] Fallback ke peer lain jika request gagal
- [ ] Simpan chunk ke state yang sama
- [ ] Reconstruct jika semua chunk selesai

Dependency:

- [ ] T62
- [ ] T55

Definition of Done:

- [ ] Satu download bisa memakai lebih dari satu sumber, walau belum parallel

---

## Phase 11 — Parallel Swarm Download

### T64 — Chunk Job Queue

- [ ] Membuat queue missing chunks
- [ ] Menghindari duplicate download chunk yang sama
- [ ] Mendukung retry
- [ ] Mendukung failed peer backoff sederhana

Dependency:

- [ ] T63

Definition of Done:

- [ ] Missing chunks bisa dibagi ke worker

---

### T65 — Parallel Worker Pool

- [ ] Menambahkan opsi `--parallel <N>`
- [ ] Spawn worker async
- [ ] Setiap worker memilih peer yang punya chunk
- [ ] Worker request chunk dan verify BLAKE3

Dependency:

- [ ] T64

Definition of Done:

- [ ] Beberapa chunk bisa didownload bersamaan

---

### T66 — Safe Concurrent State Writes

- [ ] Menulis chunk secara aman dari banyak worker
- [ ] Update progress tanpa race logical
- [ ] Hindari corrupt progress.bin
- [ ] Flush state secara periodik

Dependency:

- [ ] T65

Definition of Done:

- [ ] Parallel download tidak merusak state lokal

---

### T67 — Partial Seeder Mode

- [ ] Peer yang memiliki sebagian chunk bisa melayani chunk tersebut
- [ ] Peer mengiklankan `Have`
- [ ] Peer menolak chunk yang belum dimiliki

Dependency:

- [ ] T61
- [ ] T66

Definition of Done:

- [ ] Peer parsial bisa menjadi sumber chunk untuk peer lain

---

### T68 — Multi-peer Parallel Download

- [ ] Peer C mengambil chunk berbeda dari Seeder A dan Peer B
- [ ] Download tetap valid jika salah satu peer mati
- [ ] Verify final descriptor/file hashes

Dependency:

- [ ] T65
- [ ] T67

Definition of Done:

- [ ] Download parallel dari banyak peer berhasil

---

### T69 — Swarm Progress Logging

- [ ] Menampilkan progress global
- [ ] Menampilkan peer source per chunk saat verbose
- [ ] Menampilkan failed/retry peer saat verbose
- [ ] Menampilkan completed chunks dan throughput sederhana

Dependency:

- [ ] T68

Definition of Done:

- [ ] User bisa memahami progress swarm tanpa log terlalu bising

---

### T70 — Multi-peer Demo

- [ ] Menjalankan seeder utama
- [ ] Menjalankan peer B yang download sebagian/selesai
- [ ] Menjalankan peer C
- [ ] Peer C mengambil sebagian chunk dari seeder dan sebagian dari peer B
- [ ] File/folder akhir valid

Dependency:

- [ ] T68
- [ ] T69

Definition of Done:

- [ ] Demo swarm sederhana berhasil

---

# App Service dan GUI, dikerjakan setelah cli stabil

## Phase 12 — App Service Abstraction

### T71 — Define AppCommand

- [ ] Membuat enum `AppCommand`
- [ ] Menambahkan command `CreateShare`
- [ ] Menambahkan command `StartSeeder`
- [ ] Menambahkan command `ConnectPeer`
- [ ] Menambahkan command `DownloadShare`
- [ ] Menambahkan command `ResumeShare`
- [ ] Menambahkan command `StopTransfer`

Dependency:

- [ ] T70

Definition of Done:

- [ ] Command cukup untuk CLI dan GUI

---

### T72 — Define AppEvent

- [ ] Membuat enum `AppEvent`
- [ ] Menambahkan event `ShareCreated`
- [ ] Menambahkan event `SeederStarted`
- [ ] Menambahkan event `PeerConnected`
- [ ] Menambahkan event `KeyExchangeCompleted`
- [ ] Menambahkan event `DescriptorReceived`
- [ ] Menambahkan event `ChunkProgress`
- [ ] Menambahkan event `SwarmProgress`
- [ ] Menambahkan event `TransferCompleted`
- [ ] Menambahkan event `Error`

Dependency:

- [ ] T71

Definition of Done:

- [ ] Event cukup untuk CLI progress dan GUI update

---

### T73 — Implement AppService

- [ ] Membuat service yang menerima `AppCommand`
- [ ] Membuat service yang mengirim `AppEvent`
- [ ] Menjalankan network/swarm task secara async
- [ ] Menyediakan channel command/event

Dependency:

- [ ] T71
- [ ] T72

Definition of Done:

- [ ] Core transfer bisa dikontrol lewat AppService

---

### T74 — Refactor CLI to AppService

- [ ] CLI tidak langsung memanggil network layer
- [ ] CLI mengirim `AppCommand`
- [ ] CLI menerima dan mencetak `AppEvent`

Dependency:

- [ ] T73

Definition of Done:

- [ ] CLI tetap bekerja setelah refactor

---

### T75 — AppService Tests

- [ ] Test create share via AppCommand
- [ ] Test start seeder via AppCommand
- [ ] Test connect/download via AppCommand
- [ ] Test transfer progress via AppEvent
- [ ] Test error propagation

Dependency:

- [ ] T74

Definition of Done:

- [ ] AppService stabil dan bisa dipakai GUI

---

## Phase 13 — GUI

### T76 — Setup GUI Binary

- [ ] Membuat `src/bin/etle-gui.rs`
- [ ] Menambahkan feature `gui-relm4`
- [ ] Memastikan GUI binary bisa dibuild

Dependency:

- [ ] T75

Definition of Done:

- [ ] `cargo run --features gui-relm4 --bin etle-gui` berjalan

---

### T77 — Main Window

- [ ] Membuat window utama
- [ ] Membuat layout dasar
- [ ] Menambahkan title aplikasi

Dependency:

- [ ] T76

Definition of Done:

- [ ] Window kosong tampil

---

### T78 — Share Creator UI

- [ ] File/folder picker
- [ ] Input output descriptor `.etle`
- [ ] Tombol create share
- [ ] Menampilkan share_id

Dependency:

- [ ] T77
- [ ] T71

Definition of Done:

- [ ] User bisa membuat descriptor dari GUI

---

### T79 — Address Form UI

- [ ] Input listen address
- [ ] Input peer address list
- [ ] Input parallel worker count
- [ ] Validasi format address

Dependency:

- [ ] T77

Definition of Done:

- [ ] User bisa mengisi address seeder/peer

---

### T80 — Start Seeder Button

- [ ] Tombol start seeder
- [ ] Mengirim `AppCommand::StartSeeder`
- [ ] Menampilkan event `SeederStarted`

Dependency:

- [ ] T73
- [ ] T78
- [ ] T79

Definition of Done:

- [ ] Seeder bisa dinyalakan dari GUI

---

### T81 — Download UI

- [ ] Pilih descriptor `.etle`
- [ ] Input output folder
- [ ] Input peer list
- [ ] Tombol download/resume
- [ ] Mengirim `AppCommand::DownloadShare`

Dependency:

- [ ] T73
- [ ] T78
- [ ] T79

Definition of Done:

- [ ] Download bisa dimulai dari GUI

---

### T82 — Transfer Progress UI

- [ ] Menampilkan nama share
- [ ] Menampilkan jumlah file
- [ ] Menampilkan jumlah chunk selesai
- [ ] Menampilkan progress bar
- [ ] Update progress dari `AppEvent::ChunkProgress` dan `SwarmProgress`

Dependency:

- [ ] T72
- [ ] T73

Definition of Done:

- [ ] Progress transfer tampil real-time

---

### T83 — Log Panel UI

- [ ] Menampilkan event koneksi
- [ ] Menampilkan event key exchange
- [ ] Menampilkan event descriptor/key wrap
- [ ] Menampilkan event hash verification
- [ ] Menampilkan error

Dependency:

- [ ] T72
- [ ] T73

Definition of Done:

- [ ] Log proses transfer terlihat di GUI

---

### T84 — GUI Swarm Flow

- [ ] GUI dapat create descriptor
- [ ] GUI dapat start seeder
- [ ] GUI dapat download dari banyak peer
- [ ] GUI dapat menampilkan progress
- [ ] GUI menampilkan transfer completed

Dependency:

- [ ] T80
- [ ] T81
- [ ] T82
- [ ] T83

Definition of Done:

- [ ] File/folder bisa didownload melalui GUI

---

### T85 — GUI Manual Integration Test

- [ ] Terminal/GUI pertama menjalankan seeder
- [ ] Peer B download dan seed
- [ ] GUI kedua download dari lebih dari satu peer
- [ ] File/folder berhasil dikirim
- [ ] Progress terlihat
- [ ] Hash akhir cocok

Dependency:

- [ ] T84

Definition of Done:

- [ ] Demo GUI sukses

---

# Priority

## Selesai

- [x] T00 — Inisialisasi Project Rust
- [x] T01 — Setup Module Structure
- [x] T02 — Setup Cargo Features dan Multi Binary
- [x] T03 — Implement BLAKE3 Hashing
- [x] T04 — Implement XChaCha20-Poly1305 AEAD
- [x] T05 — Implement X25519 Key Exchange
- [x] T06 — Implement Key Derivation
- [x] T07 — Implement AAD Format
- [x] T08 — Crypto Unit Tests
- [x] T09 — Implement File Chunker
- [x] T10 — Implement Chunk Metadata
- [x] T11 — Implement Manifest Structure
- [x] T12 — Manifest Serialization
- [x] T13 — Implement Encrypted Chunk Storage
- [x] T14 — Local Reconstruct Test
- [x] T15 — Define Wire Message
- [x] T16 — Implement Protocol Codec
- [x] T17 — Protocol Serialization Tests
- [x] T18 — Implement TCP Listener
- [x] T19 — Implement TCP Client Connect
- [x] T20 — Implement Hello Handshake
- [x] T21 — Implement Network Key Exchange baseline
- [x] T22 — Test Two Peer Connection
- [x] T23 — Seeder Load File and Manifest
- [x] T24 — Peer Request/Receive Manifest
- [x] T25 — Peer Request Chunk
- [x] T26 — Seeder Send Encrypted Chunk
- [x] T27 — Peer Verify BLAKE3
- [x] T28 — Peer Decrypt Chunk
- [x] T29 — Peer Reconstruct Output File
- [x] T30 — Verify Final File Hash
- [x] T31 — CLI Command `seed`
- [x] T32 — CLI Command `connect`
- [x] T33 — CLI Command `download`
- [x] T34 — CLI Progress Logging
- [x] T35 — CLI End-to-End Demo

## Wajib Berikutnya — Torrent-like MVP

- [ ] T36 — Define ShareId
- [ ] T37 — Define EtleDescriptor
- [ ] T38 — Define FileEntry dan Logical Stream
- [ ] T39 — Descriptor Serialization
- [ ] T40 — Package Builder untuk File dan Folder
- [ ] T41 — Package Reconstruction
- [ ] T42 — Descriptor CLI Create Command
- [ ] T43 — Descriptor Tests
- [ ] T44 — Generate Reusable File Key
- [ ] T45 — Define EtleSecret
- [ ] T46 — Derive Session Key for Wrapping
- [ ] T47 — Implement Wrapped File Key
- [ ] T48 — Protocol Message `WrappedFileKey`
- [ ] T49 — Refactor Transfer to Reusable Encrypted Chunks
- [ ] T50 — Reusable Ciphertext Tests
- [ ] T51 — Define Library Paths
- [ ] T52 — Define ShareState dan ShareMode
- [ ] T53 — Implement Progress Bitmap/List
- [ ] T54 — Persist Seed State
- [ ] T55 — Persist Download State
- [ ] T56 — Seed From State
- [ ] T57 — Auto-seed After Download
- [ ] T58 — State Tests

## Wajib untuk Swarm Sederhana

- [ ] T59 — CLI Multiple Peers
- [ ] T60 — Peer Session Metadata
- [ ] T61 — Implement Have Query/Response
- [ ] T62 — Chunk Availability Map
- [ ] T63 — Multi-peer Sequential Download
- [ ] T64 — Chunk Job Queue
- [ ] T65 — Parallel Worker Pool
- [ ] T66 — Safe Concurrent State Writes
- [ ] T67 — Partial Seeder Mode
- [ ] T68 — Multi-peer Parallel Download
- [ ] T69 — Swarm Progress Logging
- [ ] T70 — Multi-peer Demo

## Setelah cli Stabil

- [ ] T71 — Define AppCommand
- [ ] T72 — Define AppEvent
- [ ] T73 — Implement AppService
- [ ] T74 — Refactor CLI to AppService
- [ ] T75 — AppService Tests
- [ ] T76 — Setup GUI Binary
- [ ] T77 — Main Window
- [ ] T78 — Share Creator UI
- [ ] T79 — Address Form UI
- [ ] T80 — Start Seeder Button
- [ ] T81 — Download UI
- [ ] T82 — Transfer Progress UI
- [ ] T83 — Log Panel UI
- [ ] T84 — GUI Swarm Flow
- [ ] T85 — GUI Manual Integration Test

---

# Sprint Plan

## Sprint 1 — Crypto dan Chunking

Status: complete.

- [x] T00
- [x] T01
- [x] T02
- [x] T03
- [x] T04
- [x] T05
- [x] T06
- [x] T07
- [x] T08
- [x] T09
- [x] T10
- [x] T11
- [x] T12
- [x] T13
- [x] T14

Goal:

- [x] File bisa dipecah, dienkripsi, diverifikasi, didekripsi, dan direkonstruksi secara lokal

---

## Sprint 2 — Protocol dan P2P

Status: complete.

- [x] T15
- [x] T16
- [x] T17
- [x] T18
- [x] T19
- [x] T20
- [x] T21 baseline
- [x] T22
- [x] T23
- [x] T24
- [x] T25
- [x] T26
- [x] T27
- [x] T28
- [x] T29
- [x] T30

Goal:

- [x] Seeder dan peer bisa transfer encrypted chunks melalui TCP

---

## Sprint 3 — CLI Demo

Status: complete.

- [x] T31
- [x] T32
- [x] T33
- [x] T34
- [x] T35

Goal:

- [x] Demo terminal end-to-end berhasil

---

## Sprint 4 — Descriptor, Package, dan Reusable Key

- [ ] T36
- [ ] T37
- [ ] T38
- [ ] T39
- [ ] T40
- [ ] T41
- [ ] T42
- [ ] T43
- [ ] T44
- [ ] T45
- [ ] T46
- [ ] T47
- [ ] T48
- [ ] T49
- [ ] T50

Goal:

- [ ] ETLE punya `descriptor.etle` sebagai metadata share seperti `.torrent`
- [ ] Satu share bisa berisi file atau folder
- [ ] Encrypted chunks reusable antar peer
- [ ] X25519 dipakai untuk wrapping file_key, bukan membuat ciphertext per peer

---

## Sprint 5 — Persistent Library State dan Auto Seed

- [ ] T51
- [ ] T52
- [ ] T53
- [ ] T54
- [ ] T55
- [ ] T56
- [ ] T57
- [ ] T58

Goal:

- [ ] Seed/download state tersimpan di `.etle/library/<share_id>`
- [ ] Download bisa resume
- [ ] Peer yang selesai download bisa menjadi seeder

---

## Sprint 6 — Multi-peer dan Parallel Swarm

- [ ] T59
- [ ] T60
- [ ] T61
- [ ] T62
- [ ] T63
- [ ] T64
- [ ] T65
- [ ] T66
- [ ] T67
- [ ] T68
- [ ] T69
- [ ] T70

Goal:

- [ ] Peer bisa mengambil chunk dari lebih dari satu peer
- [ ] Download chunk bisa parallel
- [ ] Partial seeder bekerja

---

## Sprint 7 — App Service Abstraction

- [ ] T71
- [ ] T72
- [ ] T73
- [ ] T74
- [ ] T75

Goal:

- [ ] CLI dan GUI bisa memakai satu AppService yang sama

---

## Sprint 8 — GUI

- [ ] T76
- [ ] T77
- [ ] T78
- [ ] T79
- [ ] T80
- [ ] T81
- [ ] T82
- [ ] T83
- [ ] T84
- [ ] T85

Goal:

- [ ] GUI bisa create share, start seeder, download dari peer, menampilkan progress, dan log kriptografi/swarm

---

# Definition of Done Torrent-like MVP

- [ ] CLI bisa membuat descriptor `.etle` dari file
- [ ] CLI bisa membuat descriptor `.etle` dari folder
- [ ] Descriptor tidak menyimpan file_key rahasia
- [ ] Secret key tersimpan terpisah sebagai `secret.etlekey`
- [ ] Encrypted chunks tersimpan di `.etle/library/<share_id>/chunks/`
- [ ] Seeder bisa melayani peer dari library state
- [ ] Peer bisa receive descriptor/manifest
- [ ] X25519 key exchange berhasil
- [ ] File key dikirim sebagai wrapped key, bukan plaintext
- [ ] Peer bisa unwrap file key
- [ ] Setiap chunk diverifikasi dengan BLAKE3
- [ ] File/folder bisa direkonstruksi
- [ ] Download state bisa resume
- [ ] Peer yang selesai download bisa seed ulang
- [ ] Peer bisa download dari lebih dari satu peer
- [ ] Parallel chunk download berjalan
- [ ] Final hash per file cocok

Command target awal Sprint 4/5:

```bash
cargo run --bin etle-cli -- create ./sample-folder --output sample.etle
```

```bash
cargo run --bin etle-cli -- seed sample.etle --listen 127.0.0.1:7000
```

```bash
cargo run --bin etle-cli -- download sample.etle --peer 127.0.0.1:7000 --output ./received-folder
```

Command target Sprint 6:

```bash
cargo run --bin etle-cli -- download sample.etle \
  --peer 127.0.0.1:7000 \
  --peer 127.0.0.1:7001 \
  --parallel 4 \
  --output ./received-folder
```

Expected output:

```text
[+] descriptor loaded
[+] connected to peer 127.0.0.1:7000
[+] key exchange completed
[+] wrapped file key received
[+] file key unwrapped
[+] have map received
[+] chunk 0 received from 127.0.0.1:7000
[+] BLAKE3 verification OK
[+] progress saved
[+] file/folder reconstructed
[+] final hashes matched
[+] share is now seedable
```

---

# Final Target

- [ ] Rust core library selesai
- [ ] CLI torrent-like demo selesai
- [ ] Descriptor `.etle` bekerja
- [ ] Multi-file package bekerja
- [ ] Persistent library state bekerja
- [ ] Resume download bekerja
- [ ] Auto-seed after download bekerja
- [ ] Multi-peer sequential download bekerja
- [ ] Parallel chunk download bekerja
- [ ] Partial seeder bekerja
- [ ] GUI demo selesai setelah core swarm stabil
- [ ] Secure P2P transfer bekerja
- [ ] X25519 key exchange bekerja
- [ ] XChaCha20-Poly1305 per chunk/key-wrap bekerja
- [ ] BLAKE3 per chunk/file/share bekerja
- [ ] File/folder output identik dengan input
