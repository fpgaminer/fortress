# Fortress
Remember one password, securely manage the rest.  Fortress is insanely secure and automatically synced between devices.

## Status
Fortress is being actively developed.  I use it myself.  It's not quite ready for prime time yet though and comes with lots of sharp corners users might cut their fingers on.

## Project goals

* Automatic sync - Easy to use across devices and easy backups.
* Strong security - Cracking even weak master passwords would cost millions of dollars.
* Written in Rust - Less bugs, cleaner code.
* Simple design - I opt for simplicity over performance; easier to audit the code and design.
* Trustless - The code is open source.  Data sync is end-to-end encrypted.

## Development

During development, the main `fortress` program can be run using `cargo tauri dev -- --no-default-features -- --dir [SOMEPATH]`. It includes hot-reloading.

The `fortresscrypto` crate implements all the crypto stuff unique to Fortress.  `libfortress` implements the bulk of Fortress's functionality.  `fortress` is the main binary, mainly implementing the UI.

Don't forget the usual: `cargo +nightly fmt`, `cargo clippy`, `cargo test`.

## Database Format

At its core, Fortress uses encrypted JSON, because JSON is simple, portable, and human readable.

A Fortress database consists of a collection of Objects, each of which is either a Directory or an Entry.  A Directory is just a list of other objects.  This builds a directory tree.  An Entry is basically just a HashMap, making it easy to adapt the database to new features in the future.

Every object in a Fortress database stores a timestamped history so users can roll back to previous passwords and undo mistakes.  The implementation of all Objects is designed in an append-only fashion, to ensure user data is never lost.

Using standard formats like JSON means that Fortress databases can be manipulated using existing tooling; even on the Linux command line.  Though this won't be common it's useful to have if, for example, someone wants to write third-party tools that work with Fortress databases.  Or if users want to migrate to a different password manager.

The only caveat is encryption.  There's no good, standard encryption format.  So Fortress has to use its own, but again it's very simple.  On the command line Fortress can be used to encrypt/decrypt payloads using its encryption format, so it's still possible to easily get at the JSON inside a database.

## Encryption

Fortress uses scrypt to derive encryption keys from the user's username and password, and then a construction of ChaCha20 and HMAC-SHA-512 to both encrypt and authenticate user data on disk and when performing sync.  See [ENCRYPTION.md](ENCRYPTION.md) for lots of details.

## Fortress Server

A backend "Fortress Server" facilitates syncing between devices.  It's implemented in a separate repository, [fortress-server](https://github.com/fpgaminer/fortress-server).

## Important Note

If, in rare cases, a user wishes to have multiple different Fortress databases, they must use a different username for each database.  (Don't confuse this with the common usage of a single database synced between multiple devices.)  Fortress and Fortress Server don't have a "Database ID" or anything like that.  So the only way to distinguish between different databases is to use different usernames.