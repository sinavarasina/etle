# ETLE PLAN

## 0. Scope

- [ ] Membuat prototipe transfer file/pesan multimedia berbasis P2P
- [ ] Menggunakan konsep torrent-like: seeder, peer, chunking, dan distribusi chunk
- [ ] Fokus utama pada kriptografi: XChaCha20-Poly1305, BLAKE3, dan X25519
- [ ] Tidak menargetkan kompatibilitas penuh dengan BitTorrent standar
- [ ] Menyediakan CLI untuk testing dan demo
- [ ] Menyediakan GUI untuk presentasi/demo visual

---

## 1. Arsitektur Target

- [ ] Core logic dipisahkan dari CLI dan GUI
- [ ] `lib.rs` menjadi pusat library internal
- [ ] CLI memakai core library yang sama dengan GUI
- [ ] GUI hanya berinteraksi melalui `AppCommand` dan `AppEvent`
- [ ] Transfer file berjalan asynchronous agar UI tidak freeze

```text
GUI / CLI
   ↓
App Service
   ↓
P2P Network Layer
   ↓
Protocol Message Layer
   ↓
Chunk Storage
   ↓
Crypto Layer
```

---

## 2. Struktur Direktori

- [ ] Membuat struktur project seperti berikut:

```text
p2p-crypto-transfer/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   │
│   ├── crypto/
│   │   ├── mod.rs
│   │   ├── aead.rs
│   │   ├── hash.rs
│   │   └── key_exchange.rs
│   │
│   ├── file/
│   │   ├── mod.rs
│   │   ├── chunker.rs
│   │   ├── manifest.rs
│   │   └── storage.rs
│   │
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── message.rs
│   │   └── codec.rs
│   │
│   ├── network/
│   │   ├── mod.rs
│   │   ├── peer.rs
│   │   ├── seeder.rs
│   │   └── swarm.rs
│   │
│   ├── app/
│   │   ├── mod.rs
│   │   ├── command.rs
│   │   ├── event.rs
│   │   └── service.rs
│   │
│   ├── gui/
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   ├── message.rs
│   │   └── components.rs
│   │
│   └── bin/
│       ├── p2p-cli.rs
│       └── p2p-gui.rs
│
├── tests/
│   ├── crypto_test.rs
│   ├── chunk_test.rs
│   ├── manifest_test.rs
│   └── transfer_test.rs
│
└── examples/
    └── local_transfer.rs
```

---

## 3. Dependency Awal

- [ ] Setup dependency async runtime
  - Dependency: T00
- [ ] Setup dependency serialization
  - Dependency: T00
- [ ] Setup dependency kriptografi
  - Dependency: T00
- [ ] Setup dependency CLI
  - Dependency: T00
- [ ] Setup dependency logging
  - Dependency: T00
- [ ] Setup dependency GUI sebagai optional feature
  - Dependency: T00

Target dependency:

```toml
[dependencies]
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
bincode = "1"
blake3 = "1"
chacha20poly1305 = "0.10"
x25519-dalek = "2"
rand_core = "0.6"
clap = { version = "4", features = ["derive"], optional = true }
tracing = "0.1"
tracing-subscriber = "0.3"
relm4 = { version = "0.9", optional = true }
gtk4 = { version = "0.9", optional = true }
```

---

# Task List

## Phase 0 — Project Setup

### T00 — Inisialisasi Project Rust

- [ ] Membuat project Rust baru
- [ ] Menentukan nama crate
- [ ] Menggunakan Rust edition terbaru yang stabil di environment tim
- [ ] Memastikan `cargo build` berhasil

Dependency:

- None

Definition of Done:

- [ ] `cargo build` sukses
- [ ] `cargo test` sukses walau belum ada test

---

### T01 — Setup Module Structure

- [ ] Membuat `src/lib.rs`
- [ ] Membuat folder `crypto/`
- [ ] Membuat folder `file/`
- [ ] Membuat folder `protocol/`
- [ ] Membuat folder `network/`
- [ ] Membuat folder `app/`
- [ ] Membuat folder `gui/`
- [ ] Membuat folder `src/bin/`

Dependency:

- [ ] T00

Definition of Done:

- [ ] Semua module bisa di-import dari `lib.rs`
- [ ] `cargo check` sukses

---

### T02 — Setup Cargo Features dan Multi Binary

- [ ] Menambahkan binary `p2p-cli`
- [ ] Menambahkan binary `p2p-gui`
- [ ] Membuat feature `cli`
- [ ] Membuat feature `gui-relm4`
- [ ] Membuat GUI dependency optional

Dependency:

- [ ] T00
- [ ] T01

Definition of Done:

- [ ] `cargo run --bin p2p-cli -- --help` bisa dijalankan
- [ ] `cargo run --features gui-relm4 --bin p2p-gui` bisa dibuild minimal

---

## Phase 1 — Crypto Core

### T03 — Implement BLAKE3 Hashing

- [ ] Membuat fungsi hash untuk bytes
- [ ] Membuat fungsi hash untuk file
- [ ] Membuat fungsi hash untuk chunk
- [ ] Membuat tipe `FileId`
- [ ] Membuat tipe `ChunkHash`

Dependency:

- [ ] T01
- [ ] T02

Definition of Done:

- [ ] Hash input yang sama menghasilkan output yang sama
- [ ] Hash input berbeda menghasilkan output berbeda
- [ ] Unit test hashing sukses

---

### T04 — Implement XChaCha20-Poly1305 AEAD

- [ ] Membuat fungsi encrypt chunk
- [ ] Membuat fungsi decrypt chunk
- [ ] Membuat generator nonce 24-byte
- [ ] Memastikan nonce berbeda untuk setiap chunk
- [ ] Memastikan authentication tag ikut dalam ciphertext

Dependency:

- [ ] T01
- [ ] T02

Definition of Done:

- [ ] Plaintext bisa dienkripsi dan didekripsi kembali
- [ ] Ciphertext yang diubah harus gagal decrypt
- [ ] Nonce salah harus gagal decrypt

---

### T05 — Implement X25519 Key Exchange

- [ ] Membuat ephemeral keypair
- [ ] Mengekspor public key
- [ ] Menghitung shared secret
- [ ] Memastikan dua peer menghasilkan shared secret yang sama

Dependency:

- [ ] T01
- [ ] T02

Definition of Done:

- [ ] Peer A dan Peer B menghasilkan shared secret identik
- [ ] Unit test key exchange sukses

---

### T06 — Implement Key Derivation

- [ ] Membuat fungsi derivasi `file_key`
- [ ] Input derivasi memakai shared secret
- [ ] Input derivasi memakai `file_id`
- [ ] Output derivasi berupa 32-byte key

Dependency:

- [ ] T03
- [ ] T05

Definition of Done:

- [ ] Shared secret dan file ID yang sama menghasilkan key yang sama
- [ ] File ID berbeda menghasilkan key berbeda

---

### T07 — Implement AAD Format

- [ ] Membuat format AAD untuk AEAD
- [ ] AAD memuat `file_id`
- [ ] AAD memuat `chunk_index`
- [ ] AAD memuat `chunk_size`
- [ ] AAD dipakai saat encrypt dan decrypt

Dependency:

- [ ] T04
- [ ] T06

Definition of Done:

- [ ] Chunk dengan AAD benar berhasil decrypt
- [ ] Chunk dengan `chunk_index` salah gagal decrypt
- [ ] Chunk dengan `file_id` salah gagal decrypt

---

### T08 — Crypto Unit Tests

- [ ] Test BLAKE3 hash
- [ ] Test AEAD encrypt/decrypt
- [ ] Test tampered ciphertext
- [ ] Test wrong nonce
- [ ] Test wrong AAD
- [ ] Test X25519 shared secret
- [ ] Test key derivation

Dependency:

- [ ] T03
- [ ] T04
- [ ] T05
- [ ] T06
- [ ] T07

Definition of Done:

- [ ] Semua test crypto sukses

---

## Phase 2 — File Chunking dan Manifest

### T09 — Implement File Chunker

- [ ] Membaca file dari disk
- [ ] Memecah file menjadi chunk
- [ ] Mendukung chunk size configurable
- [ ] Menjaga urutan chunk berdasarkan index

Dependency:

- [ ] T01

Definition of Done:

- [ ] File kecil bisa dipecah menjadi beberapa chunk
- [ ] File besar bisa dipecah menjadi banyak chunk
- [ ] Chunk terakhir boleh lebih kecil dari chunk size

---

### T10 — Implement Chunk Metadata

- [ ] Membuat struct `ChunkMeta`
- [ ] Menyimpan `index`
- [ ] Menyimpan `plain_size`
- [ ] Menyimpan `encrypted_size`
- [ ] Menyimpan `nonce`
- [ ] Menyimpan `blake3_hash`

Dependency:

- [ ] T03
- [ ] T09

Definition of Done:

- [ ] Metadata terbentuk untuk setiap chunk
- [ ] Metadata sesuai dengan chunk yang dibuat

---

### T11 — Implement Manifest Structure

- [ ] Membuat struct `Manifest`
- [ ] Menyimpan `file_id`
- [ ] Menyimpan `file_name`
- [ ] Menyimpan `file_size`
- [ ] Menyimpan `chunk_size`
- [ ] Menyimpan list `ChunkMeta`

Dependency:

- [ ] T10

Definition of Done:

- [ ] Manifest bisa dibuat dari file input
- [ ] Manifest memiliki jumlah chunk yang benar

---

### T12 — Manifest Serialization

- [ ] Serialize manifest ke binary
- [ ] Deserialize manifest dari binary
- [ ] Menambahkan test roundtrip serialization

Dependency:

- [ ] T02
- [ ] T11

Definition of Done:

- [ ] Manifest sebelum serialize sama dengan manifest setelah deserialize

---

### T13 — Implement Encrypted Chunk Storage

- [ ] Menyimpan encrypted chunk ke storage sementara
- [ ] Membaca encrypted chunk berdasarkan index
- [ ] Menyimpan nonce dan metadata di manifest
- [ ] Memastikan data plaintext tidak perlu disimpan setelah encrypt

Dependency:

- [ ] T04
- [ ] T11

Definition of Done:

- [ ] Encrypted chunk bisa disimpan
- [ ] Encrypted chunk bisa dibaca ulang
- [ ] Metadata chunk tetap valid

---

### T14 — Local Reconstruct Test

- [ ] File dipecah menjadi chunk
- [ ] Chunk dienkripsi
- [ ] Chunk diverifikasi
- [ ] Chunk didekripsi
- [ ] File disusun ulang
- [ ] Hash file hasil dibandingkan dengan hash file awal

Dependency:

- [ ] T08
- [ ] T09
- [ ] T10
- [ ] T11
- [ ] T12
- [ ] T13

Definition of Done:

- [ ] `BLAKE3(input_file) == BLAKE3(output_file)`

---

## Phase 3 — Protocol Message

### T15 — Define Wire Message

- [ ] Membuat enum `WireMessage`
- [ ] Menambahkan message `Hello`
- [ ] Menambahkan message `KeyExchange`
- [ ] Menambahkan message `RequestManifest`
- [ ] Menambahkan message `Manifest`
- [ ] Menambahkan message `Have`
- [ ] Menambahkan message `RequestChunk`
- [ ] Menambahkan message `Chunk`
- [ ] Menambahkan message `Error`

Dependency:

- [ ] T11
- [ ] T12

Definition of Done:

- [ ] Semua message bisa di-serialize
- [ ] Semua message bisa di-deserialize

---

### T16 — Implement Protocol Codec

- [ ] Membuat framing message
- [ ] Menambahkan length-prefix encoding
- [ ] Membuat fungsi send message
- [ ] Membuat fungsi receive message
- [ ] Menangani error decode

Dependency:

- [ ] T02
- [ ] T15

Definition of Done:

- [ ] Message bisa dikirim lewat stream lokal
- [ ] Message yang diterima sama dengan message yang dikirim

---

### T17 — Protocol Serialization Tests

- [ ] Test `Hello`
- [ ] Test `KeyExchange`
- [ ] Test `Manifest`
- [ ] Test `RequestChunk`
- [ ] Test `Chunk`
- [ ] Test invalid message handling

Dependency:

- [ ] T16

Definition of Done:

- [ ] Semua protocol test sukses

---

## Phase 4 — Basic P2P Network

### T18 — Implement TCP Listener

- [ ] Membuat seeder listener
- [ ] Menerima koneksi peer
- [ ] Spawn task untuk setiap koneksi
- [ ] Logging koneksi masuk

Dependency:

- [ ] T16

Definition of Done:

- [ ] Seeder bisa listen di address tertentu
- [ ] Seeder tidak block saat menerima peer baru

---

### T19 — Implement TCP Client Connect

- [ ] Membuat peer client
- [ ] Connect ke seeder address
- [ ] Mengirim message awal
- [ ] Logging status koneksi

Dependency:

- [ ] T16

Definition of Done:

- [ ] Peer bisa connect ke seeder lokal

---

### T20 — Implement Hello Handshake

- [ ] Peer mengirim `Hello`
- [ ] Seeder membalas `Hello`
- [ ] Menyimpan `peer_id`
- [ ] Menolak koneksi invalid

Dependency:

- [ ] T18
- [ ] T19

Definition of Done:

- [ ] Seeder dan peer saling mengetahui `peer_id`

---

### T21 — Implement Network Key Exchange

- [ ] Peer membuat ephemeral X25519 keypair
- [ ] Seeder membuat ephemeral X25519 keypair
- [ ] Keduanya bertukar public key via `KeyExchange`
- [ ] Keduanya derive `file_key`

Dependency:

- [ ] T05
- [ ] T06
- [ ] T16
- [ ] T20

Definition of Done:

- [ ] Seeder dan peer memiliki file/session key yang sama
- [ ] Key tidak dikirim langsung melalui network

---

### T22 — Test Two Peer Connection

- [ ] Jalankan seeder lokal
- [ ] Jalankan peer lokal
- [ ] Peer connect ke seeder
- [ ] Hello handshake sukses
- [ ] Key exchange sukses

Dependency:

- [ ] T18
- [ ] T19
- [ ] T20
- [ ] T21

Definition of Done:

- [ ] Log menunjukkan koneksi dan key exchange berhasil

---

## Phase 5 — Seeder dan Peer Transfer

### T23 — Seeder Load File and Manifest

- [ ] Seeder menerima path file
- [ ] Seeder melakukan chunking
- [ ] Seeder melakukan hashing
- [ ] Seeder melakukan encryption
- [ ] Seeder membuat manifest
- [ ] Seeder siap melayani request manifest dan chunk

Dependency:

- [ ] T14
- [ ] T18

Definition of Done:

- [ ] Seeder siap dengan encrypted chunks dan manifest

---

### T24 — Peer Request Manifest

- [ ] Peer mengirim `RequestManifest`
- [ ] Seeder mengirim `Manifest`
- [ ] Peer menyimpan manifest
- [ ] Peer menampilkan info file

Dependency:

- [ ] T17
- [ ] T22
- [ ] T23

Definition of Done:

- [ ] Peer menerima manifest dengan benar

---

### T25 — Peer Request Chunk

- [ ] Peer membaca daftar chunk dari manifest
- [ ] Peer mengirim `RequestChunk`
- [ ] Peer meminta chunk berdasarkan index

Dependency:

- [ ] T24

Definition of Done:

- [ ] Seeder menerima request chunk yang valid

---

### T26 — Seeder Send Encrypted Chunk

- [ ] Seeder mencari encrypted chunk berdasarkan index
- [ ] Seeder mengirim message `Chunk`
- [ ] Seeder menangani request invalid

Dependency:

- [ ] T23
- [ ] T25

Definition of Done:

- [ ] Peer menerima encrypted chunk dari seeder

---

### T27 — Peer Verify BLAKE3

- [ ] Peer menghitung hash chunk yang diterima
- [ ] Peer membandingkan dengan hash di manifest
- [ ] Peer menolak chunk yang hash-nya tidak cocok

Dependency:

- [ ] T03
- [ ] T26

Definition of Done:

- [ ] Chunk valid diterima
- [ ] Chunk rusak ditolak

---

### T28 — Peer Decrypt Chunk

- [ ] Peer membuat AAD berdasarkan manifest
- [ ] Peer decrypt chunk dengan XChaCha20-Poly1305
- [ ] Peer menolak chunk jika AEAD tag invalid

Dependency:

- [ ] T04
- [ ] T06
- [ ] T07
- [ ] T21
- [ ] T27

Definition of Done:

- [ ] Chunk valid bisa didekripsi
- [ ] Chunk dengan AAD salah gagal decrypt

---

### T29 — Peer Reconstruct Output File

- [ ] Peer menyimpan plaintext chunk berdasarkan index
- [ ] Peer menulis ulang file sesuai urutan chunk
- [ ] Peer menangani chunk terakhir yang ukurannya lebih kecil

Dependency:

- [ ] T14
- [ ] T28

Definition of Done:

- [ ] File output berhasil dibuat

---

### T30 — Verify Final File Hash

- [ ] Peer menghitung BLAKE3 file output
- [ ] Peer membandingkan dengan `file_id`
- [ ] Peer menandai transfer complete jika hash cocok

Dependency:

- [ ] T03
- [ ] T29

Definition of Done:

- [ ] `BLAKE3(input_file) == BLAKE3(output_file)`

---

## Phase 6 — CLI App

### T31 — CLI Command `seed`

- [ ] Menambahkan command `seed`
- [ ] Parameter file path
- [ ] Parameter listen address
- [ ] Menjalankan seeder dari terminal

Dependency:

- [ ] T23

Definition of Done:

- [ ] `p2p-cli seed ./sample.mp4 --listen 0.0.0.0:7000` berjalan

---

### T32 — CLI Command `connect`

- [ ] Menambahkan command `connect`
- [ ] Parameter peer address
- [ ] Melakukan koneksi ke seeder
- [ ] Melakukan handshake
- [ ] Melakukan key exchange

Dependency:

- [ ] T22

Definition of Done:

- [ ] `p2p-cli connect --peer 127.0.0.1:7000` berjalan

---

### T33 — CLI Command `download`

- [ ] Menambahkan command `download`
- [ ] Parameter peer address
- [ ] Parameter output path
- [ ] Request manifest
- [ ] Download semua chunk
- [ ] Reconstruct file
- [ ] Verify final hash

Dependency:

- [ ] T30

Definition of Done:

- [ ] File bisa didownload dari seeder melalui CLI

---

### T34 — CLI Progress Logging

- [ ] Menampilkan koneksi peer
- [ ] Menampilkan status key exchange
- [ ] Menampilkan progress chunk
- [ ] Menampilkan status BLAKE3 verification
- [ ] Menampilkan status AEAD decryption
- [ ] Menampilkan status final hash

Dependency:

- [ ] T30

Definition of Done:

- [ ] Progress transfer terlihat jelas di terminal

---

### T35 — CLI End-to-End Demo

- [ ] Menjalankan seeder dari terminal pertama
- [ ] Menjalankan peer dari terminal kedua
- [ ] Peer mendownload file
- [ ] File output sama dengan file input
- [ ] Log menunjukkan crypto verification sukses

Dependency:

- [ ] T31
- [ ] T32
- [ ] T33
- [ ] T34

Definition of Done:

- [ ] Demo CLI berhasil dari awal sampai akhir

---

## Phase 7 — App Service Abstraction

### T36 — Define AppCommand

- [ ] Membuat enum `AppCommand`
- [ ] Menambahkan command `StartSeeder`
- [ ] Menambahkan command `ConnectPeer`
- [ ] Menambahkan command `DownloadFile`
- [ ] Menambahkan command `StopTransfer`

Dependency:

- [ ] T35

Definition of Done:

- [ ] Command cukup untuk CLI dan GUI

---

### T37 — Define AppEvent

- [ ] Membuat enum `AppEvent`
- [ ] Menambahkan event `SeederStarted`
- [ ] Menambahkan event `PeerConnected`
- [ ] Menambahkan event `KeyExchangeCompleted`
- [ ] Menambahkan event `ManifestReceived`
- [ ] Menambahkan event `ChunkProgress`
- [ ] Menambahkan event `TransferCompleted`
- [ ] Menambahkan event `Error`

Dependency:

- [ ] T35

Definition of Done:

- [ ] Event cukup untuk CLI progress dan GUI update

---

### T38 — Implement AppService

- [ ] Membuat service yang menerima `AppCommand`
- [ ] Membuat service yang mengirim `AppEvent`
- [ ] Menjalankan network task secara async
- [ ] Menyediakan channel command/event

Dependency:

- [ ] T36
- [ ] T37

Definition of Done:

- [ ] Core transfer bisa dikontrol lewat AppService

---

### T39 — Refactor CLI to AppService

- [ ] CLI tidak langsung memanggil network layer
- [ ] CLI mengirim `AppCommand`
- [ ] CLI menerima dan mencetak `AppEvent`

Dependency:

- [ ] T38

Definition of Done:

- [ ] CLI tetap bekerja setelah refactor

---

### T40 — AppService Tests

- [ ] Test start seeder via AppCommand
- [ ] Test connect peer via AppCommand
- [ ] Test transfer progress via AppEvent
- [ ] Test error propagation

Dependency:

- [ ] T39

Definition of Done:

- [ ] AppService stabil dan bisa dipakai GUI

---

## Phase 8 — GUI

### T41 — Setup GUI Binary

- [ ] Membuat `src/bin/p2p-gui.rs`
- [ ] Menambahkan feature `gui-relm4`
- [ ] Memastikan GUI binary bisa dibuild

Dependency:

- [ ] T40

Definition of Done:

- [ ] `cargo run --features gui-relm4 --bin p2p-gui` berjalan

---

### T42 — Main Window

- [ ] Membuat window utama
- [ ] Membuat layout dasar
- [ ] Menambahkan title aplikasi

Dependency:

- [ ] T41

Definition of Done:

- [ ] Window kosong tampil

---

### T43 — File Picker UI

- [ ] Menambahkan tombol pilih file
- [ ] Menampilkan path file terpilih
- [ ] Validasi file path

Dependency:

- [ ] T42

Definition of Done:

- [ ] User bisa memilih file dari GUI

---

### T44 — Address Form UI

- [ ] Input listen address
- [ ] Input peer address
- [ ] Validasi format address

Dependency:

- [ ] T42

Definition of Done:

- [ ] User bisa mengisi address seeder/peer

---

### T45 — Start Seeder Button

- [ ] Tombol start seeder
- [ ] Mengirim `AppCommand::StartSeeder`
- [ ] Menampilkan event `SeederStarted`

Dependency:

- [ ] T38
- [ ] T43
- [ ] T44

Definition of Done:

- [ ] Seeder bisa dinyalakan dari GUI

---

### T46 — Connect Peer Button

- [ ] Tombol connect peer
- [ ] Mengirim `AppCommand::ConnectPeer`
- [ ] Menampilkan event `PeerConnected`
- [ ] Menampilkan event `KeyExchangeCompleted`

Dependency:

- [ ] T38
- [ ] T44

Definition of Done:

- [ ] Peer bisa connect dari GUI

---

### T47 — Transfer Progress UI

- [ ] Menampilkan nama file
- [ ] Menampilkan jumlah chunk selesai
- [ ] Menampilkan progress bar
- [ ] Update progress dari `AppEvent::ChunkProgress`

Dependency:

- [ ] T37
- [ ] T38

Definition of Done:

- [ ] Progress transfer tampil real-time

---

### T48 — Log Panel UI

- [ ] Menampilkan event koneksi
- [ ] Menampilkan event key exchange
- [ ] Menampilkan event hash verification
- [ ] Menampilkan event decrypt success/failure
- [ ] Menampilkan error

Dependency:

- [ ] T37
- [ ] T38

Definition of Done:

- [ ] Log proses transfer terlihat di GUI

---

### T49 — GUI Download Flow

- [ ] GUI dapat connect ke seeder
- [ ] GUI dapat request manifest
- [ ] GUI dapat download chunk
- [ ] GUI dapat menampilkan progress
- [ ] GUI menampilkan transfer completed

Dependency:

- [ ] T45
- [ ] T46
- [ ] T47
- [ ] T48

Definition of Done:

- [ ] File bisa didownload melalui GUI

---

### T50 — GUI Manual Integration Test

- [ ] Terminal/GUI pertama menjalankan seeder
- [ ] GUI kedua connect sebagai peer
- [ ] File berhasil dikirim
- [ ] Progress terlihat
- [ ] Hash akhir cocok

Dependency:

- [ ] T49

Definition of Done:

- [ ] Demo GUI sukses

---

## Phase 9 — Multi-Peer / Swarm Sederhana

### T51 — Peer Registry

- [ ] Menyimpan daftar peer aktif
- [ ] Menyimpan status koneksi peer
- [ ] Menyimpan peer ID

Dependency:

- [ ] T30

Definition of Done:

- [ ] Daftar peer aktif bisa dilihat

---

### T52 — Implement Have Message

- [ ] Peer mengirim daftar chunk yang dimiliki
- [ ] Seeder menerima daftar chunk peer
- [ ] Peer lain bisa mengetahui chunk availability

Dependency:

- [ ] T15
- [ ] T51

Definition of Done:

- [ ] Message `Have` berfungsi

---

### T53 — Chunk Availability Map

- [ ] Membuat map `chunk_index -> peer list`
- [ ] Update map saat menerima `Have`
- [ ] Memilih peer berdasarkan chunk yang tersedia

Dependency:

- [ ] T52

Definition of Done:

- [ ] Sistem tahu peer mana punya chunk tertentu

---

### T54 — Download Chunk from Multiple Peers

- [ ] Request chunk berbeda dari peer berbeda
- [ ] Menangani peer gagal
- [ ] Fallback ke peer lain jika request gagal

Dependency:

- [ ] T53

Definition of Done:

- [ ] Peer bisa download chunk dari lebih dari satu sumber

---

### T55 — Partial Seeder Mode

- [ ] Peer yang sudah punya chunk dapat melayani request chunk
- [ ] Peer mengirim `Have` setelah chunk berhasil diverifikasi
- [ ] Peer lain bisa download dari peer parsial

Dependency:

- [ ] T54

Definition of Done:

- [ ] Peer bisa menjadi seeder parsial

---

### T56 — Multi-Peer Demo

- [ ] Menjalankan seeder utama
- [ ] Menjalankan peer B
- [ ] Menjalankan peer C
- [ ] Peer C mengambil sebagian chunk dari seeder dan sebagian dari peer B
- [ ] File akhir tetap valid

Dependency:

- [ ] T55

Definition of Done:

- [ ] Demo swarm sederhana berhasil

---

# Priority

## Wajib untuk MVP

- [ ] T00 — Inisialisasi Project Rust
- [ ] T01 — Setup Module Structure
- [ ] T02 — Setup Cargo Features dan Multi Binary
- [ ] T03 — Implement BLAKE3 Hashing
- [ ] T04 — Implement XChaCha20-Poly1305 AEAD
- [ ] T05 — Implement X25519 Key Exchange
- [ ] T06 — Implement Key Derivation
- [ ] T07 — Implement AAD Format
- [ ] T08 — Crypto Unit Tests
- [ ] T09 — Implement File Chunker
- [ ] T10 — Implement Chunk Metadata
- [ ] T11 — Implement Manifest Structure
- [ ] T12 — Manifest Serialization
- [ ] T13 — Implement Encrypted Chunk Storage
- [ ] T14 — Local Reconstruct Test
- [ ] T15 — Define Wire Message
- [ ] T16 — Implement Protocol Codec
- [ ] T17 — Protocol Serialization Tests
- [ ] T18 — Implement TCP Listener
- [ ] T19 — Implement TCP Client Connect
- [ ] T20 — Implement Hello Handshake
- [ ] T21 — Implement Network Key Exchange
- [ ] T22 — Test Two Peer Connection
- [ ] T23 — Seeder Load File and Manifest
- [ ] T24 — Peer Request Manifest
- [ ] T25 — Peer Request Chunk
- [ ] T26 — Seeder Send Encrypted Chunk
- [ ] T27 — Peer Verify BLAKE3
- [ ] T28 — Peer Decrypt Chunk
- [ ] T29 — Peer Reconstruct Output File
- [ ] T30 — Verify Final File Hash
- [ ] T31 — CLI Command `seed`
- [ ] T32 — CLI Command `connect`
- [ ] T33 — CLI Command `download`
- [ ] T34 — CLI Progress Logging
- [ ] T35 — CLI End-to-End Demo

## Wajib untuk GUI

- [ ] T36 — Define AppCommand
- [ ] T37 — Define AppEvent
- [ ] T38 — Implement AppService
- [ ] T39 — Refactor CLI to AppService
- [ ] T40 — AppService Tests
- [ ] T41 — Setup GUI Binary
- [ ] T42 — Main Window
- [ ] T43 — File Picker UI
- [ ] T44 — Address Form UI
- [ ] T45 — Start Seeder Button
- [ ] T46 — Connect Peer Button
- [ ] T47 — Transfer Progress UI
- [ ] T48 — Log Panel UI
- [ ] T49 — GUI Download Flow
- [ ] T50 — GUI Manual Integration Test

## Opsional / Enhancement

- [ ] T51 — Peer Registry
- [ ] T52 — Implement Have Message
- [ ] T53 — Chunk Availability Map
- [ ] T54 — Download Chunk from Multiple Peers
- [ ] T55 — Partial Seeder Mode
- [ ] T56 — Multi-Peer Demo

---

# Sprint Plan

## Sprint 1 — Crypto dan Chunking

- [ ] T00
- [ ] T01
- [ ] T02
- [ ] T03
- [ ] T04
- [ ] T05
- [ ] T06
- [ ] T07
- [ ] T08
- [ ] T09
- [ ] T10
- [ ] T11
- [ ] T12
- [ ] T13
- [ ] T14

Goal:

- [ ] File bisa dipecah, dienkripsi, diverifikasi, didekripsi, dan direkonstruksi secara lokal

---

## Sprint 2 — Protocol dan P2P

- [ ] T15
- [ ] T16
- [ ] T17
- [ ] T18
- [ ] T19
- [ ] T20
- [ ] T21
- [ ] T22
- [ ] T23
- [ ] T24
- [ ] T25
- [ ] T26
- [ ] T27
- [ ] T28
- [ ] T29
- [ ] T30

Goal:

- [ ] Seeder dan peer bisa transfer encrypted chunks melalui TCP

---

## Sprint 3 — CLI Demo

- [ ] T31
- [ ] T32
- [ ] T33
- [ ] T34
- [ ] T35

Goal:

- [ ] Demo terminal end-to-end berhasil

---

## Sprint 4 — GUI

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

- [ ] GUI bisa memilih file, start seeder, connect peer, transfer file, menampilkan progress, dan menampilkan log kriptografi

---

## Sprint 5 — Swarm Sederhana

- [ ] T51
- [ ] T52
- [ ] T53
- [ ] T54
- [ ] T55
- [ ] T56

Goal:

- [ ] Peer dapat mengambil chunk dari lebih dari satu peer

---

# Definition of Done MVP

- [ ] CLI bisa menjalankan seeder
- [ ] CLI bisa menjalankan peer
- [ ] Peer bisa connect ke seeder
- [ ] Key exchange berhasil
- [ ] Manifest diterima peer
- [ ] File dikirim sebagai encrypted chunks
- [ ] Setiap chunk diverifikasi dengan BLAKE3
- [ ] Setiap chunk didekripsi dengan XChaCha20-Poly1305
- [ ] File berhasil direkonstruksi
- [ ] Hash file output sama dengan hash file input

Command target:

```bash
cargo run --bin p2p-cli -- seed ./sample.mp4 --listen 0.0.0.0:7000
```

```bash
cargo run --bin p2p-cli -- download --peer 127.0.0.1:7000 --output ./received.mp4
```

Expected output:

```text
[+] connected to peer
[+] key exchange completed
[+] manifest received
[+] chunk 0 received
[+] BLAKE3 verification OK
[+] AEAD decryption OK
[+] file reconstructed
[+] final file hash matched
```

---

# Non-Goals

- [ ] Tidak mengejar BitTorrent compatibility
- [ ] Tidak mengejar public DHT
- [ ] Tidak mengejar NAT traversal penuh
- [ ] Tidak mengejar tracker kompleks
- [ ] Tidak mengejar account system
- [ ] Tidak mengejar anonymous routing
- [ ] Tidak mengejar mobile app production-ready
- [ ] Tidak mengejar chat bubble UI kompleks sebelum transfer file stabil

---

# Final Target

- [ ] Rust core library selesai
- [ ] CLI demo selesai
- [ ] GUI demo selesai
- [ ] Secure P2P transfer bekerja
- [ ] Seeder dan peer bekerja
- [ ] Chunking bekerja
- [ ] X25519 key exchange bekerja
- [ ] XChaCha20-Poly1305 per chunk bekerja
- [ ] BLAKE3 per chunk/file bekerja
- [ ] Manifest custom bekerja
- [ ] File output identik dengan file input
